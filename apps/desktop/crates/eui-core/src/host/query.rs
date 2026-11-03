//! VAPP-189 (S1): THE records query, the Rust twin of
//! `packages/exponential-ui/src/host/query.ts`, locked by
//! `fixtures/records-query.json` (`parseCases`, `sourceCases`, `jsonCases`,
//! `valueCases`, `expandCases`; `bucketCases` lock [`crate::dates`]). A
//! binding's URL sugar (`records:<kind>?q`) parses INTO the structured
//! [`RecordsQuery`]; [`records_value`] evaluates it over the rows of any
//! store. The desktop host calls it directly; the Swift and Compose SDKs
//! reach it through `exponential-ui-ffi` (`parse_records_query_json`,
//! `parse_records_json`, `parse_records_source_json`, `records_value_json`).
//!
//! VAPP-190 (Q1): the JSON form ([`parse_records_json`], a package's
//! `queries`, the `q=<json>` param): `group` (any field, `author`, date
//! buckets), `aggregate`, `running`, `before`, `in` / `between` and `or`.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::signature::canonical_json;
use super::sources::{parse_source, source_with_params, ParsedSource};
use super::{js_trim, utf16_cmp};
use crate::dates::{date_bucket, days_in_month, weekday_of, DateBucket, DATE_BUCKETS};
use crate::json::{number, number_to_string};

/// The binding scheme: `records:<kind>[?query]`.
pub const RECORDS_SCHEME: &str = "records";
/// The URL sugar's defaults and caps (catalog/host.json `package.records.query`).
pub const RECORDS_SORT_DEFAULT: &str = "createdAt";
pub const RECORDS_ORDER_DEFAULT: &str = "desc";
pub const RECORDS_LIMIT_DEFAULT: u32 = 100;
pub const RECORDS_LIMIT_MAX: u32 = 500;
/// Query keys with a meaning of their own; never a field filter.
pub const RECORDS_RESERVED_PARAMS: &[&str] = &["app", "before", "by", "expand", "limit", "mine", "month", "order", "q", "query", "sort", "sum", "user"];
/// The keys every value row carries next to its data (a data key never shadows one).
pub const RECORDS_ROW_KEYS: &[&str] = &["id", "createdAt", "updatedAt", "createdBy", "authorId", "author"];
/// The filter operators: `eq` is the bare key, the others a `.<op>` suffix
/// (`in` and `between` take comma-separated values).
pub const RECORDS_FILTER_OPS: &[&str] = &["eq", "gt", "gte", "lt", "lte", "in", "between"];
/// The JSON form's spellings of the operators.
pub const RECORDS_OP_ALIASES: &[(&str, &str)] = &[("=", "eq"), ("==", "eq"), (">", "gt"), (">=", "gte"), ("<", "lt"), ("<=", "lte")];
pub const RECORDS_AGGREGATE_OPS: &[&str] = &["sum", "avg", "min", "max", "count", "countDistinct", "first", "last"];
pub const RECORDS_RUNNING_OPS: &[&str] = &["sum", "count", "avg", "min", "max"];
/// Names an aggregate may not take (a group's and a total's own keys).
pub const RECORDS_AGGREGATE_RESERVED: &[&str] = &["count", "sum", "running", "rows"];
/// The month view's names (en-US, the same ×4).
pub const RECORDS_WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub const RECORDS_MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordsFilterOp {
    Eq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Between,
}

impl RecordsFilterOp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
            Self::In => "in",
            Self::Between => "between",
        }
    }

    /// An operator by name (`eq` … `between`) or its JSON spelling (`<`, `>=`, `==`…).
    pub fn parse(text: &str) -> Option<Self> {
        let name = RECORDS_OP_ALIASES.iter().find(|(alias, _)| *alias == text).map_or(text, |(_, op)| op);
        match name {
            "eq" => Some(Self::Eq),
            "gt" => Some(Self::Gt),
            "gte" => Some(Self::Gte),
            "lt" => Some(Self::Lt),
            "lte" => Some(Self::Lte),
            "in" => Some(Self::In),
            "between" => Some(Self::Between),
            _ => None,
        }
    }

    /// The suffix operators (`<field>.<op>`); `eq` is the bare key.
    fn suffix(text: &str) -> Option<Self> {
        match text {
            "gt" => Some(Self::Gt),
            "gte" => Some(Self::Gte),
            "lt" => Some(Self::Lt),
            "lte" => Some(Self::Lte),
            "in" => Some(Self::In),
            "between" => Some(Self::Between),
            _ => None,
        }
    }

    fn is_list(self) -> bool {
        matches!(self, Self::In | Self::Between)
    }
}

/// A condition's value: one text, or a list (`in`; `between` = [low, high]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RecordsFilterValue {
    One(String),
    Many(Vec<String>),
}

/// One condition: `eq` / `in` compare as text, the ranges (`between`
/// inclusive) as numbers when both sides parse, else as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsFilter {
    pub field: String,
    pub op: RecordsFilterOp,
    pub value: RecordsFilterValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordsOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsSortKey {
    pub field: String,
    pub order: RecordsOrder,
}

/// `by=<dateField>&month=YYYY-MM`: the rows of one month, every day listed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsMonthView {
    pub field: String,
    /// `YYYY-MM`; `None` = the month of `now`.
    pub key: Option<String>,
}

/// One group-by key: a field (`author` = the author, by name), or a date
/// field cut into buckets. The group's column is named `field`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsGroupKey {
    pub field: String,
    pub bucket: Option<DateBucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordsAggregateOp {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    CountDistinct,
    First,
    Last,
}

impl RecordsAggregateOp {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "sum" => Some(Self::Sum),
            "avg" => Some(Self::Avg),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "count" => Some(Self::Count),
            "countDistinct" => Some(Self::CountDistinct),
            "first" => Some(Self::First),
            "last" => Some(Self::Last),
            _ => None,
        }
    }
}

/// `name: [op, field?]`: a column of every group (and of `totals`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsAggregate {
    pub name: String,
    pub op: RecordsAggregateOp,
    /// `None` = `count` of the rows.
    pub field: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordsRunningOp {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

impl RecordsRunningOp {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "sum" => Some(Self::Sum),
            "count" => Some(Self::Count),
            "avg" => Some(Self::Avg),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

/// A cumulative value of `field` over the groups (or rows, or month days) in order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsRunning {
    pub field: String,
    pub op: RecordsRunningOp,
}

/// The rows whose `field` is before `date` are left out of the rows and
/// groups, totalled into `before` and seed every running value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsBefore {
    pub field: String,
    pub date: String,
}

/// The structured query.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsQuery {
    /// The kind; `None` = no kind (the value is null).
    pub from: Option<String>,
    /// Another app's slug (the HOST resolves whose rows); `None` = the hosting app.
    pub app: Option<String>,
    /// Only the viewer's rows (by author: `authorId`, else `createdBy`).
    pub mine: bool,
    /// Only this user's rows (by author).
    pub user: Option<String>,
    /// Every condition holds; by field, then operator (the URL sugar).
    #[serde(rename = "where")]
    pub where_: Vec<RecordsFilter>,
    /// VAPP-190: one of these alternatives holds too (each: every condition).
    #[serde(default)]
    pub or: Vec<Vec<RecordsFilter>>,
    /// Sort keys in order; ties always fall back to the id.
    pub sort: Vec<RecordsSortKey>,
    pub limit: u32,
    /// Σ of this field over the matches (before the limit).
    pub sum: Option<String>,
    pub month: Option<RecordsMonthView>,
    /// VAPP-190: the group-by keys.
    #[serde(default)]
    pub group: Vec<RecordsGroupKey>,
    /// VAPP-190: the aggregates, by name.
    #[serde(default)]
    pub aggregate: Vec<RecordsAggregate>,
    /// VAPP-190: the running values, by field.
    #[serde(default)]
    pub running: Vec<RecordsRunning>,
    #[serde(default)]
    pub before: Option<RecordsBefore>,
    /// eApps R5 (VAPP-219): ref fields whose target is inlined into every
    /// row (`records.<kind>.refs`, one level).
    #[serde(default)]
    pub expand: Vec<String>,
}

impl Default for RecordsQuery {
    fn default() -> Self {
        RecordsQuery {
            from: None,
            app: None,
            mine: false,
            user: None,
            where_: Vec::new(),
            or: Vec::new(),
            sort: vec![RecordsSortKey { field: RECORDS_SORT_DEFAULT.to_string(), order: RecordsOrder::Desc }],
            limit: RECORDS_LIMIT_DEFAULT,
            sum: None,
            month: None,
            group: Vec::new(),
            aggregate: Vec::new(),
            running: Vec::new(),
            before: None,
            expand: Vec::new(),
        }
    }
}

/// A problem with a JSON query, a JSON pointer into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordsQueryIssue {
    pub path: String,
    pub message: String,
}

/// JavaScript's `Number(string)`: trimmed, empty = 0, `0x` / `0o` / `0b`
/// integers, `[+-]Infinity`, a decimal literal; anything else NaN.
pub(crate) fn js_string_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = t.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |n, c| n * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0)));
        }
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    let b = body.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    };
    let int = digits(&mut i);
    let frac = if b.get(i) == Some(&b'.') {
        i += 1;
        digits(&mut i)
    } else {
        0
    };
    if int == 0 && frac == 0 {
        return f64::NAN;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return f64::NAN;
        }
    }
    if i != b.len() {
        return f64::NAN;
    }
    body.parse::<f64>().map(|n| sign * n).unwrap_or(f64::NAN)
}

/// `YYYY-MM` with a month 01–12.
fn is_month_key(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 7 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' && matches!(&text[5..7], "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12")
}

fn trimmed(v: Option<&str>) -> Option<String> {
    v.map(js_trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Query params (as [`super::parse_source`] gives them) → the query. `kind`
/// = the source's own name (`records:<kind>`). Unknown keys are field
/// filters. VAPP-190: `q=<json>` = the
/// JSON form ([`parse_records_json`]; the other params are ignored).
pub fn parse_records_query(params: &BTreeMap<String, String>, kind: Option<&str>) -> RecordsQuery {
    let from = trimmed(kind);
    if let Some(q) = params.get("q").filter(|q| !js_trim(q).is_empty()) {
        let json = serde_json::from_str::<Value>(q).ok().filter(Value::is_object).unwrap_or_else(|| json!({}));
        return parse_records_json(&json, from.as_deref()).0;
    }
    let limit_raw = params.get("limit").map_or(f64::NAN, |l| js_string_number(l));
    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort_by(|a, b| utf16_cmp(a, b));
    let mut where_ = Vec::new();
    for key in keys {
        if key.is_empty() || RECORDS_RESERVED_PARAMS.contains(&key.as_str()) {
            continue;
        }
        let value = params[key].clone();
        match key.rfind('.').and_then(|dot| RecordsFilterOp::suffix(&key[dot + 1..]).map(|op| (dot, op))) {
            Some((dot, op)) if dot > 0 => {
                let value = if op.is_list() { RecordsFilterValue::Many(value.split(',').map(str::to_string).collect()) } else { RecordsFilterValue::One(value) };
                where_.push(RecordsFilter { field: key[..dot].to_string(), op, value });
            }
            _ => where_.push(RecordsFilter { field: key.clone(), op: RecordsFilterOp::Eq, value: RecordsFilterValue::One(value) }),
        }
    }
    where_.sort_by(|a, b| utf16_cmp(&a.field, &b.field).then_with(|| utf16_cmp(a.op.as_str(), b.op.as_str())));
    let order = match params.get("order").map(String::as_str) {
        Some("asc") => RecordsOrder::Asc,
        Some("desc") => RecordsOrder::Desc,
        _ if RECORDS_ORDER_DEFAULT == "asc" => RecordsOrder::Asc,
        _ => RecordsOrder::Desc,
    };
    let limit = if limit_raw.is_finite() && limit_raw.fract() == 0.0 && limit_raw > 0.0 { limit_raw.min(f64::from(RECORDS_LIMIT_MAX)) as u32 } else { RECORDS_LIMIT_DEFAULT };
    let by = trimmed(params.get("by").map(String::as_str));
    let before = trimmed(params.get("before").map(String::as_str));
    RecordsQuery {
        from,
        app: trimmed(params.get("app").map(String::as_str)),
        mine: matches!(params.get("mine").map(String::as_str), Some("1" | "true")),
        user: trimmed(params.get("user").map(String::as_str)),
        where_,
        sort: vec![RecordsSortKey { field: trimmed(params.get("sort").map(String::as_str)).unwrap_or_else(|| RECORDS_SORT_DEFAULT.to_string()), order }],
        limit,
        sum: trimmed(params.get("sum").map(String::as_str)),
        before: before.map(|date| RecordsBefore { field: by.clone().unwrap_or_else(|| RECORDS_SORT_DEFAULT.to_string()), date }),
        month: by.map(|field| RecordsMonthView { field, key: params.get("month").filter(|m| is_month_key(m)).cloned() }),
        expand: {
            let mut out: Vec<String> = Vec::new();
            for f in params.get("expand").map(String::as_str).unwrap_or("").split(',').map(js_trim).filter(|f| !f.is_empty()) {
                if !out.iter().any(|o| o == f) {
                    out.push(f.to_string());
                }
            }
            out
        },
        ..RecordsQuery::default()
    }
}

/// `records:<kind>?…` → the query; `None` = not a records source.
pub fn parse_records_source(source: &ParsedSource) -> Option<RecordsQuery> {
    (source.scheme == RECORDS_SCHEME).then(|| parse_records_query(&source.params, Some(&source.name)))
}

/// A JSON scalar as the query's text (a number as JS prints it); `None` = not a scalar.
fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some(String::new()),
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(number_to_string(n.as_f64().unwrap_or(0.0))),
        _ => None,
    }
}

fn issue(issues: &mut Vec<RecordsQueryIssue>, path: impl Into<String>, message: impl Into<String>) {
    issues.push(RecordsQueryIssue { path: path.into(), message: message.into() });
}

/// `[field, op, value…]` → a condition.
fn parse_condition(entry: &Value, at: &str, issues: &mut Vec<RecordsQueryIssue>) -> Option<RecordsFilter> {
    let Some(list) = entry.as_array().filter(|l| l.len() >= 3) else {
        issue(issues, at, "a condition is [field, op, value]");
        return None;
    };
    let Some(field) = list[0].as_str().map(js_trim).filter(|f| !f.is_empty()) else {
        issue(issues, format!("{at}/0"), "a condition's field is a non-empty string");
        return None;
    };
    let Some(op) = list[1].as_str().and_then(RecordsFilterOp::parse) else {
        issue(issues, format!("{at}/1"), format!("unknown operator {}", crate::json::to_js_string(&list[1])));
        return None;
    };
    let rest = &list[2..];
    if op == RecordsFilterOp::In {
        let values: Option<Vec<String>> = match rest {
            [Value::Array(items)] => items.iter().map(scalar_text).collect(),
            _ => None,
        };
        let Some(values) = values else {
            issue(issues, format!("{at}/2"), "in takes one list of values");
            return None;
        };
        return Some(RecordsFilter { field: field.to_string(), op, value: RecordsFilterValue::Many(values) });
    }
    let values: Option<Vec<String>> = rest.iter().map(scalar_text).collect();
    let want = if op == RecordsFilterOp::Between { 2 } else { 1 };
    let Some(mut values) = values.filter(|v| v.len() == want) else {
        issue(issues, format!("{at}/2"), if op == RecordsFilterOp::Between { "between takes a low and a high value".to_string() } else { format!("{} takes one value", op.as_str()) });
        return None;
    };
    let value = if op == RecordsFilterOp::Between { RecordsFilterValue::Many(values) } else { RecordsFilterValue::One(values.remove(0)) };
    Some(RecordsFilter { field: field.to_string(), op, value })
}

const QUERY_KEYS: &[&str] = &["from", "app", "mine", "user", "where", "sort", "limit", "sum", "month", "group", "aggregate", "running", "before", "expand"];

fn sorted_keys(map: &Map<String, Value>) -> Vec<&String> {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by(|a, b| utf16_cmp(a, b));
    keys
}

/// VAPP-190: the JSON form (a package's `queries`, the `q` param) → the
/// query and every problem with it (a problem drops the part, the rest
/// still runs). `kind` (a source's own) wins over `from`.
pub fn parse_records_json(json: &Value, kind: Option<&str>) -> (RecordsQuery, Vec<RecordsQueryIssue>) {
    let mut issues = Vec::new();
    let mut query = RecordsQuery::default();
    let Some(obj) = json.as_object() else {
        issue(&mut issues, "", "a query is an object");
        query.from = trimmed(kind);
        return (query, issues);
    };
    for key in sorted_keys(obj) {
        if !QUERY_KEYS.contains(&key.as_str()) && !key.starts_with('$') {
            issue(&mut issues, format!("/{key}"), format!("unknown key {key}"));
        }
    }
    let str_of = |key: &str, issues: &mut Vec<RecordsQueryIssue>| -> Option<String> {
        match obj.get(key) {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => trimmed(Some(s)),
            Some(_) => {
                issue(issues, format!("/{key}"), format!("{key} is a string"));
                None
            }
        }
    };
    query.from = match trimmed(kind) {
        Some(k) => Some(k),
        None => str_of("from", &mut issues),
    };
    query.app = str_of("app", &mut issues);
    query.user = str_of("user", &mut issues);
    query.sum = str_of("sum", &mut issues);
    match obj.get("mine") {
        None => {}
        Some(Value::Bool(b)) => query.mine = *b,
        Some(_) => issue(&mut issues, "/mine", "mine is a boolean"),
    }
    match obj.get("where") {
        None => {}
        Some(Value::Array(list)) => {
            for (i, entry) in list.iter().enumerate() {
                let at = format!("/where/{i}");
                if let Some(alt) = entry.as_object() {
                    let Some(alternatives) = alt.get("or").and_then(Value::as_array).filter(|_| alt.len() == 1) else {
                        issue(&mut issues, at, "an alternative is {or: [[condition…]…]}");
                        continue;
                    };
                    if !query.or.is_empty() {
                        issue(&mut issues, at, "one or per query");
                        continue;
                    }
                    for (j, alt) in alternatives.iter().enumerate() {
                        let Some(conds) = alt.as_array() else {
                            issue(&mut issues, format!("{at}/or/{j}"), "an alternative is a list of conditions");
                            continue;
                        };
                        let parsed: Vec<RecordsFilter> = conds.iter().enumerate().filter_map(|(k, c)| parse_condition(c, &format!("{at}/or/{j}/{k}"), &mut issues)).collect();
                        query.or.push(parsed);
                    }
                    continue;
                }
                if let Some(parsed) = parse_condition(entry, &at, &mut issues) {
                    query.where_.push(parsed);
                }
            }
        }
        Some(_) => issue(&mut issues, "/where", "where is a list of conditions"),
    }
    if let Some(sort) = obj.get("sort") {
        let single = sort.is_string();
        let list: Option<Vec<Value>> = if single { Some(vec![sort.clone()]) } else { sort.as_array().cloned() };
        match list {
            None => issue(&mut issues, "/sort", "sort is a field or a list of [field, order]"),
            Some(list) => {
                let mut keys = Vec::new();
                for (i, entry) in list.iter().enumerate() {
                    let asc = Value::String("asc".into());
                    let (name, order): (Option<&Value>, Option<&Value>) = match entry {
                        Value::String(_) => (Some(entry), Some(&asc)),
                        Value::Array(pair) => (pair.first(), if pair.len() > 1 { pair.get(1) } else { Some(&asc) }),
                        _ => (None, None),
                    };
                    let name = name.and_then(Value::as_str).map(js_trim).filter(|n| !n.is_empty());
                    let order = match order.and_then(Value::as_str) {
                        Some("asc") => Some(RecordsOrder::Asc),
                        Some("desc") => Some(RecordsOrder::Desc),
                        _ => None,
                    };
                    match (name, order) {
                        (Some(name), Some(order)) => keys.push(RecordsSortKey { field: name.to_string(), order }),
                        _ => issue(&mut issues, if single { "/sort".to_string() } else { format!("/sort/{i}") }, "a sort key is field or [field, asc|desc]"),
                    }
                }
                if !keys.is_empty() {
                    query.sort = keys;
                }
            }
        }
    }
    if let Some(limit) = obj.get("limit") {
        let n = match limit {
            Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
            Value::String(s) if !js_trim(s).is_empty() => js_string_number(s),
            _ => f64::NAN,
        };
        if n.is_finite() && n.fract() == 0.0 && n >= 1.0 {
            query.limit = n.min(f64::from(RECORDS_LIMIT_MAX)) as u32;
        } else {
            issue(&mut issues, "/limit", "limit is a positive integer");
        }
    }
    match obj.get("month") {
        None | Some(Value::Null) => {}
        Some(m) => {
            let field = m.get("field").and_then(Value::as_str).map(js_trim).filter(|f| !f.is_empty());
            let key = m.get("key");
            match (m.as_object(), field) {
                (Some(_), Some(field)) if matches!(key, None | Some(Value::Null) | Some(Value::String(_))) => {
                    query.month = Some(RecordsMonthView { field: field.to_string(), key: key.and_then(Value::as_str).filter(|k| is_month_key(k)).map(str::to_string) });
                }
                _ => issue(&mut issues, "/month", "month is {field, key?}"),
            }
        }
    }
    match obj.get("group") {
        None => {}
        Some(Value::Array(list)) => {
            for (i, entry) in list.iter().enumerate() {
                let at = format!("/group/{i}");
                let key = match entry {
                    Value::String(s) if !js_trim(s).is_empty() => Some(RecordsGroupKey { field: js_trim(s).to_string(), bucket: None }),
                    Value::Object(o) if o.len() == 2 => {
                        let field = o.get("bucket").and_then(Value::as_str).map(js_trim).filter(|f| !f.is_empty());
                        let bucket = o.get("by").and_then(Value::as_str).and_then(DateBucket::parse);
                        field.zip(bucket).map(|(field, bucket)| RecordsGroupKey { field: field.to_string(), bucket: Some(bucket) })
                    }
                    _ => None,
                };
                let Some(key) = key else {
                    issue(&mut issues, at, format!("a group key is a field or {{bucket: field, by: {}}}", DATE_BUCKETS.join("|")));
                    continue;
                };
                if query.group.iter().any(|g| g.field == key.field || (key.field == "author" && g.field == "authorId") || (key.field == "authorId" && g.field == "author")) {
                    issue(&mut issues, at, format!("duplicate group {}", key.field));
                    continue;
                }
                query.group.push(key);
            }
        }
        Some(_) => issue(&mut issues, "/group", "group is a list of keys"),
    }
    match obj.get("aggregate") {
        None => {}
        Some(Value::Object(aggs)) => {
            for name in sorted_keys(aggs) {
                let def = &aggs[name];
                let at = format!("/aggregate/{name}");
                if js_trim(name).is_empty() || RECORDS_AGGREGATE_RESERVED.contains(&name.as_str()) || query.group.iter().any(|g| g.field == *name || (g.field == "author" && name == "authorId")) {
                    issue(&mut issues, at, format!("the name {name} is taken"));
                    continue;
                }
                let list = def.as_array();
                let op = list.and_then(|l| l.first()).and_then(Value::as_str).and_then(RecordsAggregateOp::parse);
                let len = list.map_or(0, Vec::len);
                let field = list.filter(|l| l.len() > 1).and_then(|l| l[1].as_str()).map(js_trim).filter(|f| !f.is_empty());
                match op {
                    Some(op) if len <= 2 && !(len > 1 && field.is_none()) && (op == RecordsAggregateOp::Count || field.is_some()) => {
                        query.aggregate.push(RecordsAggregate { name: name.clone(), op, field: field.map(str::to_string) });
                    }
                    _ => issue(&mut issues, at, format!("an aggregate is [{}, field] (count: the field is optional)", RECORDS_AGGREGATE_OPS.join("|"))),
                }
            }
        }
        Some(_) => issue(&mut issues, "/aggregate", "aggregate is {name: [op, field?]}"),
    }
    match obj.get("running") {
        None => {}
        Some(Value::Object(running)) => {
            for name in sorted_keys(running) {
                match running[name].as_str().and_then(RecordsRunningOp::parse) {
                    Some(op) if !js_trim(name).is_empty() => query.running.push(RecordsRunning { field: name.clone(), op }),
                    _ => issue(&mut issues, format!("/running/{name}"), format!("a running value is {{field: {}}}", RECORDS_RUNNING_OPS.join("|"))),
                }
            }
        }
        Some(_) => issue(&mut issues, "/running", "running is {field: op}"),
    }
    match obj.get("before") {
        None | Some(Value::Null) => {}
        Some(Value::String(date)) => {
            let implied = query.group.iter().find(|g| g.bucket.is_some()).map(|g| g.field.clone()).or_else(|| query.month.as_ref().map(|m| m.field.clone())).unwrap_or_else(|| RECORDS_SORT_DEFAULT.to_string());
            query.before = trimmed(Some(date)).map(|date| RecordsBefore { field: implied, date });
        }
        Some(b) => {
            let field = b.as_object().and_then(|o| o.get("field")).and_then(Value::as_str).map(js_trim).filter(|f| !f.is_empty());
            match (field, b.get("date").and_then(Value::as_str)) {
                (Some(field), Some(date)) => query.before = trimmed(Some(date)).map(|date| RecordsBefore { field: field.to_string(), date }),
                _ => issue(&mut issues, "/before", "before is a date or {field, date}"),
            }
        }
    }
    // eApps R5 (VAPP-219): the ref fields to inline.
    match obj.get("expand") {
        None => {}
        Some(Value::Array(list)) => {
            for (i, entry) in list.iter().enumerate() {
                match entry.as_str().map(js_trim).filter(|f| !f.is_empty()) {
                    None => issue(&mut issues, format!("/expand/{i}"), "an expanded field is a non-empty string"),
                    Some(f) if query.expand.iter().any(|e| e == f) => issue(&mut issues, format!("/expand/{i}"), format!("duplicate expand {f}")),
                    Some(f) => query.expand.push(f.to_string()),
                }
            }
        }
        Some(_) => issue(&mut issues, "/expand", "expand is a list of ref fields"),
    }
    (query, issues)
}

/// `${name}` in every string of a JSON query → `params[name]` ("" when unset).
pub fn fill_query_templates(value: &Value, params: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(s) => {
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                let Some(len) = rest[start + 2..].find('}') else { break };
                out.push_str(&rest[..start]);
                let name = js_trim(&rest[start + 2..start + 2 + len]);
                out.push_str(params.get(name).map_or("", String::as_str));
                rest = &rest[start + 2 + len + 1..];
            }
            out.push_str(rest);
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| fill_query_templates(v, params)).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), fill_query_templates(v, params))).collect()),
        other => other.clone(),
    }
}

/// VAPP-190: a package binding `<scheme>:<name>?query=<id>&<params>` naming
/// one of the package's `queries` → the same source with that query, its
/// `${param}` templates filled from the other params, as `q=<canonical
/// JSON>` (a `kind` param stays). Anything else is returned as written.
pub fn expand_query_source(source: &str, queries: Option<&Value>) -> String {
    let Some(parsed) = parse_source(source) else { return source.to_string() };
    let Some(id) = parsed.params.get("query") else { return source.to_string() };
    let Some(query) = queries.and_then(Value::as_object).and_then(|q| q.get(id)) else { return source.to_string() };
    let mut params = parsed.params.clone();
    params.remove("query");
    let filled = fill_query_templates(query, &params);
    let base = js_trim(source);
    let head = base.split_once('?').map_or(base, |(h, _)| h);
    source_with_params(head, Some(&json!({ "q": canonical_json(&filled) })))
}

/// One stored record as a host hands it to the evaluator (borrowed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordRowRef<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub data: &'a Value,
    pub created_by: Option<&'a str>,
    /// Whose record it is (#VAPP-187); `None` = `created_by`.
    pub author_id: Option<&'a str>,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

impl<'a> RecordRowRef<'a> {
    /// The row's author: `authorId`, else `createdBy`.
    pub fn author(&self) -> Option<&'a str> {
        self.author_id.or(self.created_by)
    }
}

/// One stored record, owned (the FFI's rows JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordRow {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl RecordRow {
    pub fn as_ref(&self) -> RecordRowRef<'_> {
        RecordRowRef {
            id: &self.id,
            kind: &self.kind,
            data: &self.data,
            created_by: self.created_by.as_deref(),
            author_id: self.author_id.as_deref(),
            created_at: &self.created_at,
            updated_at: &self.updated_at,
        }
    }
}

/// What [`records_value`] needs besides the rows.
#[derive(Debug, Clone, Copy)]
pub struct RecordsContext<'a> {
    /// The viewer's user id (`None` = signed out: `mine` matches nothing).
    pub me: Option<&'a str>,
    /// ISO 8601 (UTC): the clock (computed fields).
    pub now: &'a str,
    /// `YYYY-MM-DD`: the host's calendar day in the team zone (team-zone.json):
    /// the month view's `today` and default month; `None` = `now`'s UTC day.
    pub today: Option<&'a str>,
    /// user id → display name (`author`; "" when unknown).
    pub names: &'a HashMap<String, String>,
    /// VAPP-214 (Q3): the queried kind's computed fields (`records.<kind>.computed`:
    /// name → CEL), evaluated on every row of the kind before the query.
    pub computed: Option<&'a BTreeMap<String, String>>,
    /// eApps R5 (VAPP-219): the queried kind's refs (`records.<kind>.refs`:
    /// field → `{kind}` | `"issue"` | `"member"`), what `expand` follows.
    pub refs: Option<&'a Value>,
    /// eApps R5: identifier → the issue summary an `issue` ref expands to.
    pub issues: Option<&'a Map<String, Value>>,
}

/// eApps R5 (VAPP-219): a package's refs of `kind` (`records.<kind>.refs`,
/// the well-formed ones); `None` = none. What a host passes as `refs`.
pub fn kind_refs(records: Option<&Value>, kind: Option<&str>) -> Option<Value> {
    let def = records?.as_object()?.get(kind?)?.as_object()?;
    let mut out = Map::new();
    for (name, target) in def.get("refs")?.as_object()? {
        match target {
            Value::String(t) if t == "issue" || t == "member" => {
                out.insert(name.clone(), target.clone());
            }
            Value::Object(o) => {
                if let Some(k) = o.get("kind").and_then(Value::as_str).filter(|k| !k.is_empty()) {
                    out.insert(name.clone(), json!({ "kind": k }));
                }
            }
            _ => {}
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

/// A record as a value row: its data and the row keys (`author` = the name).
fn value_row(row: &RecordRowRef, names: &HashMap<String, String>) -> Map<String, Value> {
    let mut obj: Map<String, Value> = row.data.as_object().cloned().unwrap_or_default();
    obj.insert("id".into(), json!(row.id));
    obj.insert("createdAt".into(), json!(row.created_at));
    obj.insert("updatedAt".into(), json!(row.updated_at));
    obj.insert("createdBy".into(), json!(row.created_by));
    obj.insert("authorId".into(), json!(row.author()));
    let author = row.author().and_then(|a| names.get(a)).cloned().unwrap_or_default();
    obj.insert("author".into(), json!(author));
    obj
}

/// eApps R5 (VAPP-219): one ref's value inlined: a record of the target kind
/// among the rows (as a value row, its own refs not followed), an issue
/// summary from `ctx.issues`, a member `{id, name}` from `ctx.names`; a
/// dangling ref and an empty one read null.
fn expanded_ref(target: &Value, value: Option<&Value>, by_id: &HashMap<&str, &RecordRowRef>, ctx: &RecordsContext) -> Value {
    let id = match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(0.0)),
        _ => String::new(),
    };
    if id.is_empty() {
        return Value::Null;
    }
    match target {
        Value::String(t) if t == "issue" => ctx.issues.and_then(|m| m.get(&id)).cloned().unwrap_or(Value::Null),
        Value::String(t) if t == "member" => ctx.names.get(&id).map_or(Value::Null, |name| json!({ "id": id, "name": name })),
        _ => {
            let kind = target.get("kind").and_then(Value::as_str).unwrap_or("");
            by_id.get(id.as_str()).filter(|r| r.kind == kind).map_or(Value::Null, |r| Value::Object(value_row(r, ctx.names)))
        }
    }
}

/// VAPP-214 (Q3): at most this many computed fields per kind.
pub const RECORDS_COMPUTED_MAX: usize = 16;
/// The row keys a computed field reads next to its kind's fields (`author`, a name, never).
pub const RECORDS_COMPUTED_ROW_KEYS: [&str; 5] = ["id", "createdAt", "updatedAt", "createdBy", "authorId"];

/// VAPP-214 (Q3): a package's computed fields of `kind` (`records.<kind>.computed`,
/// the string ones); `None` = none. What a host passes as `computed`.
pub fn kind_computed(records: Option<&Value>, kind: Option<&str>) -> Option<BTreeMap<String, String>> {
    let def = records?.as_object()?.get(kind?)?.as_object()?;
    let out: BTreeMap<String, String> = def.get("computed")?.as_object()?.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect();
    (!out.is_empty()).then_some(out)
}

/// VAPP-214 (Q3): the data of every row of `kind` with its computed fields
/// written in (`None` for other kinds' rows: they stay as they are). A field
/// reads the row's data and the row keys by name (a missing one = null); it
/// never reads another computed field (the checker refuses it). A failing
/// or unparsable expression reads null. Evaluated in UTC with the query's
/// `now`, like the query's own buckets. Never stored.
pub fn computed_data(rows: &[RecordRowRef], kind: &str, computed: &BTreeMap<String, String>, now: &str) -> Vec<Option<Value>> {
    let fields: Vec<(&String, Option<crate::cel::Node>, Vec<String>)> = computed
        .iter()
        .map(|(name, source)| {
            let ast = crate::cel::parse_expression(source).ok();
            let reads = ast.as_ref().map(crate::cel::expression_names).unwrap_or_default();
            (name, ast, reads)
        })
        .collect();
    let env = crate::cel::ExpressionEnv { now: Some(crate::cel::parse_timestamp(now).unwrap_or(0.0)), offset: None, max_steps: None };
    rows.iter()
        .map(|row| {
            if row.kind != kind {
                return None;
            }
            let mut vars: Map<String, Value> = row.data.as_object().cloned().unwrap_or_default();
            vars.insert("id".into(), Value::String(row.id.to_string()));
            vars.insert("createdAt".into(), Value::String(row.created_at.to_string()));
            vars.insert("updatedAt".into(), Value::String(row.updated_at.to_string()));
            vars.insert("createdBy".into(), row.created_by.map_or(Value::Null, |s| Value::String(s.to_string())));
            vars.insert("authorId".into(), row.author().map_or(Value::Null, |s| Value::String(s.to_string())));
            let mut data: Map<String, Value> = row.data.as_object().cloned().unwrap_or_default();
            for (name, ast, reads) in &fields {
                for r in reads {
                    if !vars.contains_key(r) {
                        vars.insert(r.clone(), Value::Null);
                    }
                }
                let value = ast.as_ref().and_then(|ast| crate::cel::evaluate_node(ast, &vars, &env).ok()).unwrap_or(Value::Null);
                data.insert((*name).clone(), value);
            }
            Some(Value::Object(data))
        })
        .collect()
}

/// A field as a JS value (`undefined` = `Missing`).
#[derive(Clone, Copy)]
enum Field<'a> {
    Text(&'a str),
    Json(&'a Value),
    Missing,
}

fn num_text(s: &str) -> Option<f64> {
    if js_trim(s).is_empty() {
        return None;
    }
    Some(js_string_number(s)).filter(|f| f.is_finite())
}

fn num(v: &Field) -> Option<f64> {
    match v {
        Field::Json(Value::Number(n)) => n.as_f64().filter(|f| f.is_finite()),
        Field::Json(Value::String(s)) => num_text(s),
        Field::Text(s) => num_text(s),
        Field::Json(_) | Field::Missing => None,
    }
}

fn num_value(v: Option<&Value>) -> Option<f64> {
    v.and_then(|v| num(&Field::Json(v)))
}

/// VAPP-202: the field as text, BORROWED when it is one (strings, the row
/// keys); only numbers, booleans and objects allocate.
fn text<'a>(v: &Field<'a>) -> Cow<'a, str> {
    match *v {
        Field::Missing | Field::Json(Value::Null) => Cow::Borrowed(""),
        Field::Text(s) => Cow::Borrowed(s),
        Field::Json(Value::String(s)) => Cow::Borrowed(s.as_str()),
        Field::Json(Value::Bool(b)) => Cow::Borrowed(if *b { "true" } else { "false" }),
        Field::Json(Value::Number(n)) => Cow::Owned(number_to_string(n.as_f64().unwrap_or(0.0))),
        Field::Json(other) => Cow::Owned(canonical_json(other)),
    }
}

/// The field as JSON (`undefined` = null).
fn json_of(v: &Field) -> Value {
    match v {
        Field::Missing => Value::Null,
        Field::Text(s) => Value::String((*s).to_string()),
        Field::Json(v) => (*v).clone(),
    }
}

/// VAPP-202: a field read ONCE for comparing (a sort key, a filter's bound):
/// its number when it parses, else its text. [`cmp_keys`] = [`compare`]
/// without parsing or stringifying per comparison.
enum Key<'a> {
    /// A number; the field stays for the text side of a mixed comparison.
    Num(f64, Field<'a>),
    Text(Cow<'a, str>),
}

impl<'a> Key<'a> {
    fn of(v: Field<'a>) -> Self {
        match num(&v) {
            Some(n) => Key::Num(n, v),
            None => Key::Text(text(&v)),
        }
    }

    /// A text key's text ("" for a number: [`Col::of`] only asks text keys).
    fn text_ref(&self) -> &str {
        match self {
            Key::Num(..) => "",
            Key::Text(t) => t,
        }
    }

    fn text(&self) -> Cow<'_, str> {
        match self {
            Key::Num(_, f) => text(f),
            Key::Text(t) => Cow::Borrowed(t),
        }
    }
}

/// Numbers when both parse, else text (UTF-16 code units, no locale).
fn cmp_keys(a: &Key, b: &Key) -> Ordering {
    if let (Key::Num(na, _), Key::Num(nb, _)) = (a, b) {
        return na.partial_cmp(nb).unwrap_or(Ordering::Equal);
    }
    utf16_cmp(&a.text(), &b.text())
}

fn compare(a: &Field, b: &Field) -> Ordering {
    cmp_keys(&Key::of(*a), &Key::of(*b))
}

/// VAPP-202: a condition with its bounds parsed once per query, not per row.
struct Cond<'q> {
    filter: &'q RecordsFilter,
    bounds: Vec<Key<'q>>,
}

impl<'q> Cond<'q> {
    fn new(filter: &'q RecordsFilter) -> Self {
        let bounds = match (&filter.value, filter.op) {
            (_, RecordsFilterOp::Eq | RecordsFilterOp::In) => Vec::new(),
            (RecordsFilterValue::One(v), _) => vec![Key::of(Field::Text(v))],
            (RecordsFilterValue::Many(vs), _) => vs.iter().map(|v| Key::of(Field::Text(v))).collect(),
        };
        Cond { filter, bounds }
    }

    fn holds(&self, row: &RecordRowRef) -> bool {
        let value = field(row, &self.filter.field);
        match self.filter.op {
            RecordsFilterOp::Eq => match &self.filter.value {
                RecordsFilterValue::One(v) => text(&value) == v.as_str(),
                RecordsFilterValue::Many(_) => false,
            },
            RecordsFilterOp::In => {
                let t = text(&value);
                match &self.filter.value {
                    RecordsFilterValue::One(v) => t == v.as_str(),
                    RecordsFilterValue::Many(vs) => vs.iter().any(|v| t == v.as_str()),
                }
            }
            RecordsFilterOp::Between => self.bounds.len() == 2 && {
                let k = Key::of(value);
                cmp_keys(&k, &self.bounds[0]).is_ge() && cmp_keys(&k, &self.bounds[1]).is_le()
            },
            op => {
                let [bound] = self.bounds.as_slice() else { return false };
                let c = cmp_keys(&Key::of(value), bound);
                match op {
                    RecordsFilterOp::Gt => c.is_gt(),
                    RecordsFilterOp::Gte => c.is_ge(),
                    RecordsFilterOp::Lt => c.is_lt(),
                    _ => c.is_le(),
                }
            }
        }
    }
}

fn data_field<'a>(row: &RecordRowRef<'a>, name: &str) -> Field<'a> {
    match row.data.as_object().and_then(|o| o.get(name)) {
        Some(v) => Field::Json(v),
        None => Field::Missing,
    }
}

static NULL: Value = Value::Null;

/// A field as the query reads it: a row key (`authorId` = the author), else the data's.
fn field<'a>(row: &RecordRowRef<'a>, name: &str) -> Field<'a> {
    match name {
        "createdAt" => Field::Text(row.created_at),
        "updatedAt" => Field::Text(row.updated_at),
        "id" => Field::Text(row.id),
        "createdBy" => row.created_by.map_or(Field::Json(&NULL), Field::Text),
        "authorId" => row.author().map_or(Field::Json(&NULL), Field::Text),
        _ => data_field(row, name),
    }
}

fn parse_month(key: &str) -> (i64, i64) {
    let (y, m) = key.split_once('-').unwrap_or(("1970", "1"));
    (y.parse().unwrap_or(1970), m.parse::<i64>().unwrap_or(1).clamp(1, 12))
}

/// `YYYY-MM` ± `by` months.
pub fn shift_month(key: &str, by: i64) -> String {
    let (y, m) = parse_month(key);
    let index = y * 12 + (m - 1) + by;
    format!("{}-{:02}", index.div_euclid(12), index.rem_euclid(12) + 1)
}

/// `October 2026`.
pub fn month_label(key: &str) -> String {
    let (y, m) = parse_month(key);
    format!("{} {y}", RECORDS_MONTHS[(m - 1) as usize])
}

/// One day of the month view.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordsDay {
    pub date: String,
    pub day: i64,
    pub weekday: &'static str,
    pub count: u64,
    pub sum: f64,
}

/// Every day of `key` (`YYYY-MM`), empty.
pub fn month_days(key: &str) -> Vec<RecordsDay> {
    let (y, m) = parse_month(key);
    (1..=days_in_month(y, m)).map(|day| RecordsDay { date: format!("{key}-{day:02}"), day, weekday: RECORDS_WEEKDAYS[weekday_of(y, m, day)], count: 0, sum: 0.0 }).collect()
}

/// JS `Math.round(n * 1e6) / 1e6`: sums like 0.1 + 0.2 stay readable (and identical ×4).
fn round(n: f64) -> f64 {
    (n * 1_000_000.0 + 0.5).floor() / 1_000_000.0
}

/// `s.slice(0, n)` (dates and ISO timestamps: ASCII).
fn prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// One aggregate over rows in the query's order.
fn aggregate_of(agg: &RecordsAggregate, rows: &[&RecordRowRef]) -> Value {
    let values: Vec<Field> = match &agg.field {
        Some(f) => rows.iter().map(|r| field(r, f)).collect(),
        None => Vec::new(),
    };
    match agg.op {
        RecordsAggregateOp::Count => json!(if agg.field.is_none() { rows.len() } else { values.iter().filter(|v| !text(v).is_empty()).count() }),
        RecordsAggregateOp::Sum => number(round(values.iter().fold(0.0, |acc, v| acc + num(v).unwrap_or(0.0)))),
        RecordsAggregateOp::Avg => {
            let nums: Vec<f64> = values.iter().filter_map(num).collect();
            if nums.is_empty() {
                Value::Null
            } else {
                number(round(nums.iter().fold(0.0, |a, b| a + b) / nums.len() as f64))
            }
        }
        RecordsAggregateOp::Min | RecordsAggregateOp::Max => {
            let mut best: Option<&Field> = None;
            for v in &values {
                if text(v).is_empty() {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some(b) if agg.op == RecordsAggregateOp::Min => compare(v, b).is_lt(),
                    Some(b) => compare(v, b).is_gt(),
                };
                if better {
                    best = Some(v);
                }
            }
            best.map_or(Value::Null, |b| num(b).map_or_else(|| Value::String(text(b).into_owned()), number))
        }
        RecordsAggregateOp::CountDistinct => {
            let mut seen: Vec<Cow<str>> = values.iter().map(text).filter(|t| !t.is_empty()).collect();
            seen.sort();
            seen.dedup();
            json!(seen.len())
        }
        RecordsAggregateOp::First => values.first().map_or(Value::Null, json_of),
        RecordsAggregateOp::Last => values.last().map_or(Value::Null, json_of),
    }
}

fn sum_of(row: &RecordRowRef, f: &str) -> f64 {
    num_value(row.data.as_object().and_then(|o| o.get(f))).unwrap_or(0.0)
}

/// count, sum?, then every aggregate: a group's, `totals`' and `before`'s columns.
fn summary(query: &RecordsQuery, rows: &[&RecordRowRef], out: &mut Map<String, Value>) {
    out.insert("count".into(), json!(rows.len()));
    if let Some(sum) = &query.sum {
        out.insert("sum".into(), number(round(rows.iter().fold(0.0, |acc, row| acc + sum_of(row, sum)))));
    }
    for agg in &query.aggregate {
        out.insert(agg.name.clone(), aggregate_of(agg, rows));
    }
}

#[derive(Default, Clone, Copy)]
struct RunningAcc {
    sum: f64,
    count: u64,
    nums: u64,
    min: Option<f64>,
    max: Option<f64>,
}

/// The running values: fed row by row, read after each step.
struct Running<'q> {
    list: &'q [RecordsRunning],
    acc: Vec<RunningAcc>,
}

impl<'q> Running<'q> {
    fn new(list: &'q [RecordsRunning]) -> Self {
        Running { list, acc: vec![RunningAcc::default(); list.len()] }
    }

    fn add(&mut self, row: &RecordRowRef) {
        for (r, a) in self.list.iter().zip(self.acc.iter_mut()) {
            a.count += 1;
            let Some(n) = num(&field(row, &r.field)) else { continue };
            a.sum += n;
            a.nums += 1;
            a.min = Some(a.min.map_or(n, |m| if n < m { n } else { m }));
            a.max = Some(a.max.map_or(n, |m| if n > m { n } else { m }));
        }
    }

    fn read(&self) -> Value {
        let mut out = Map::new();
        for (r, a) in self.list.iter().zip(&self.acc) {
            let v = match r.op {
                RecordsRunningOp::Sum => number(round(a.sum)),
                RecordsRunningOp::Count => json!(a.count),
                RecordsRunningOp::Avg if a.nums > 0 => number(round(a.sum / a.nums as f64)),
                RecordsRunningOp::Avg => Value::Null,
                RecordsRunningOp::Min => a.min.map_or(Value::Null, number),
                RecordsRunningOp::Max => a.max.map_or(Value::Null, number),
            };
            out.insert(r.field.clone(), v);
        }
        Value::Object(out)
    }
}

/// A group's part of the key: the author's id, a date's bucket key, any
/// other field's text. VAPP-202: computed once per row, borrowed when it can be.
fn group_part<'a>(key: &RecordsGroupKey, row: &RecordRowRef<'a>) -> Cow<'a, str> {
    if key.field == "author" {
        return Cow::Borrowed(row.author().unwrap_or(""));
    }
    let v = field(row, &key.field);
    match key.bucket {
        Some(bucket) => Cow::Owned(date_bucket(&text(&v), bucket).unwrap_or_default()),
        None => text(&v),
    }
}

/// A group's column (from its first row): `author` = the author's name (+
/// `authorId`), a bucket = the date's bucket key, any other field its value.
fn group_columns(key: &RecordsGroupKey, row: &RecordRowRef, names: &HashMap<String, String>, cols: &mut Map<String, Value>) {
    if key.field == "author" {
        let author = row.author();
        cols.insert("author".into(), json!(author.and_then(|a| names.get(a)).cloned().unwrap_or_default()));
        cols.insert("authorId".into(), json!(author));
        return;
    }
    let v = field(row, &key.field);
    if let Some(bucket) = key.bucket {
        cols.insert(key.field.clone(), json!(date_bucket(&text(&v), bucket)));
        return;
    }
    cols.insert(key.field.clone(), json_of(&v));
}

/// `s.slice(0, n)` borrowed (n chars).
fn prefix_str(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// One sort column, read once per row: every value a number, every value
/// text (ranked: the distinct texts sorted once, each row their index), or
/// both (the generic comparison).
enum Col<'a> {
    Num(Vec<f64>),
    Rank(Vec<u32>),
    Mixed(Vec<Key<'a>>),
}

impl<'a> Col<'a> {
    fn of(rows: &[&RecordRowRef<'a>], name: &str) -> Self {
        let keys: Vec<Key<'a>> = rows.iter().map(|row| Key::of(field(row, name))).collect();
        if keys.iter().all(|k| matches!(k, Key::Num(..))) {
            return Col::Num(keys.iter().map(|k| if let Key::Num(n, _) = k { *n } else { 0.0 }).collect());
        }
        if !keys.iter().all(|k| matches!(k, Key::Text(_))) {
            return Col::Mixed(keys);
        }
        let mut ranks: FxHashMap<&str, u32> = FxHashMap::default();
        for k in &keys {
            ranks.entry(k.text_ref()).or_insert(0);
        }
        let mut distinct: Vec<&str> = ranks.keys().copied().collect();
        distinct.sort_unstable_by(|a, b| utf16_cmp(a, b));
        for (i, t) in distinct.iter().enumerate() {
            ranks.insert(t, i as u32);
        }
        Col::Rank(keys.iter().map(|k| ranks[k.text_ref()]).collect())
    }

    fn cmp(&self, a: usize, b: usize) -> Ordering {
        match self {
            Col::Num(v) => v[a].partial_cmp(&v[b]).unwrap_or(Ordering::Equal),
            Col::Rank(v) => v[a].cmp(&v[b]),
            Col::Mixed(v) => cmp_keys(&v[a], &v[b]),
        }
    }
}

/// VAPP-202: `rows` in the query's order (sort keys, ties by id), each
/// row's keys read ONCE into columns instead of parsing and stringifying
/// both sides of every comparison. Without a mixed column the order is
/// total (the index breaks an id tie), so the unstable sort gives the
/// stable one's result.
fn sorted<'r, 'a>(rows: Vec<&'r RecordRowRef<'a>>, sort: &[RecordsSortKey]) -> Vec<&'r RecordRowRef<'a>> {
    let cols: Vec<(Col<'a>, bool)> = sort.iter().map(|s| (Col::of(&rows, &s.field), s.order == RecordsOrder::Asc)).collect();
    let ascii: Vec<bool> = rows.iter().map(|row| row.id.is_ascii()).collect();
    let order = |a: usize, b: usize| {
        for (col, asc) in &cols {
            let c = col.cmp(a, b);
            if c.is_ne() {
                return if *asc { c } else { c.reverse() };
            }
        }
        let (ia, ib) = (rows[a].id, rows[b].id);
        if ascii[a] || ascii[b] {
            ia.as_bytes().cmp(ib.as_bytes())
        } else {
            utf16_cmp(ia, ib)
        }
    };
    let mut idx: Vec<usize> = (0..rows.len()).collect();
    if cols.iter().any(|(c, _)| matches!(c, Col::Mixed(_))) {
        idx.sort_by(|&a, &b| order(a, b));
    } else {
        idx.sort_unstable_by(|&a, &b| order(a, b).then(a.cmp(&b)));
    }
    idx.into_iter().map(|i| rows[i]).collect()
}

/// The value a records binding emits for `query` over `rows` (every row of
/// the app, any kind): `None` without a kind; else {rows, count, sum?,
/// month?, groups?, totals?, before?}: the matches sorted (ties by id) then
/// cut at the limit, `count` and `sum` before the limit.
pub fn records_value(rows: &[RecordRowRef], query: &RecordsQuery, ctx: RecordsContext) -> Option<Value> {
    records_value_in(rows, rows, query, ctx)
}

/// [`records_value`] with `all` = the rows refs resolve against (before the
/// computed fields are written).
fn records_value_in(all: &[RecordRowRef], rows: &[RecordRowRef], query: &RecordsQuery, ctx: RecordsContext) -> Option<Value> {
    let kind = query.from.as_deref()?;
    // VAPP-214: the computed fields ride in each row's data from here on.
    if let Some(computed) = ctx.computed.filter(|c| !c.is_empty()) {
        let data = computed_data(rows, kind, computed, ctx.now);
        let rows: Vec<RecordRowRef> = rows.iter().zip(&data).map(|(row, d)| RecordRowRef { data: d.as_ref().unwrap_or(row.data), ..*row }).collect();
        return records_value_in(all, &rows, query, RecordsContext { computed: None, ..ctx });
    }
    // eApps R5: the expanded fields that are refs of the kind (another name stays as stored).
    let expand: Vec<(&str, &Value)> = query.expand.iter().filter_map(|name| ctx.refs.and_then(|r| r.get(name)).map(|t| (name.as_str(), t))).collect();
    let by_id: HashMap<&str, &RecordRowRef> = if expand.is_empty() { HashMap::new() } else { all.iter().map(|r| (r.id, r)).collect() };
    let view = query.month.as_ref();
    let today: &str = ctx.today.filter(|t| is_day_text(t)).unwrap_or_else(|| prefix_str(ctx.now, 10));
    let month_key: Option<String> = view.map(|v| v.key.clone().unwrap_or_else(|| prefix(today, 7)));
    let where_: Vec<Cond> = query.where_.iter().map(Cond::new).collect();
    let or: Vec<Vec<Cond>> = query.or.iter().map(|alt| alt.iter().map(Cond::new).collect()).collect();
    let before_date = query.before.as_ref().map(|b| Key::of(Field::Text(&b.date)));
    let mut before: Vec<&RecordRowRef> = Vec::new();
    let mut matched: Vec<&RecordRowRef> = Vec::new();
    for row in rows {
        if row.kind != kind {
            continue;
        }
        let author = row.author();
        if query.mine && (ctx.me.is_none() || author != ctx.me) {
            continue;
        }
        if let Some(user) = &query.user {
            if author != Some(user.as_str()) {
                continue;
            }
        }
        if !where_.iter().all(|c| c.holds(row)) {
            continue;
        }
        if !or.is_empty() && !or.iter().any(|alt| alt.iter().all(|c| c.holds(row))) {
            continue;
        }
        if let (Some(b), Some(date)) = (&query.before, &before_date) {
            let t = text(&field(row, &b.field));
            if !t.is_empty() && cmp_keys(&Key::of(Field::Text(&t)), date).is_lt() {
                before.push(row);
                continue;
            }
        }
        if let (Some(view), Some(key)) = (view, &month_key) {
            if !text(&data_field(row, &view.field)).starts_with(key.as_str()) {
                continue;
            }
        }
        matched.push(row);
    }
    let matched = sorted(matched, &query.sort);
    let before = sorted(before, &query.sort);
    let seeded = || {
        let mut r = Running::new(&query.running);
        for row in &before {
            r.add(row);
        }
        r
    };
    let mut row_running = (!query.running.is_empty() && query.group.is_empty()).then(seeded);
    let out_rows: Vec<Value> = matched
        .iter()
        .take(query.limit as usize)
        .map(|row| {
            let mut obj = value_row(row, ctx.names);
            for (name, target) in &expand {
                obj.insert((*name).to_string(), expanded_ref(target, row.data.get(*name), &by_id, &ctx));
            }
            if let Some(r) = row_running.as_mut() {
                r.add(row);
                obj.insert("running".into(), r.read());
            }
            Value::Object(obj)
        })
        .collect();
    let mut out = Map::new();
    out.insert("rows".into(), Value::Array(out_rows));
    out.insert("count".into(), json!(matched.len()));
    if let Some(sum) = &query.sum {
        let total = matched.iter().fold(0.0, |acc, row| acc + sum_of(row, sum));
        out.insert("sum".into(), number(round(total)));
    }
    if let (Some(view), Some(key)) = (view, &month_key) {
        let mut days = month_days(key);
        let mut per_day: Vec<Vec<&RecordRowRef>> = vec![Vec::new(); days.len()];
        for row in &matched {
            let t = text(&data_field(row, &view.field));
            let date = prefix_str(&t, 10);
            let Some(i) = days.iter().position(|d| d.date == date) else {
                continue;
            };
            let day = &mut days[i];
            day.count += 1;
            if let Some(sum) = &query.sum {
                day.sum = round(day.sum + sum_of(row, sum));
            }
            per_day[i].push(row);
        }
        let mut runnings: Vec<Option<Value>> = vec![None; days.len()];
        if !query.running.is_empty() {
            let mut r = seeded();
            for (i, rows) in per_day.iter().enumerate() {
                for row in rows {
                    r.add(row);
                }
                runnings[i] = Some(r.read());
            }
        }
        let labels: Vec<String> = days.iter().map(|d| d.day.to_string()).collect();
        let counts: Vec<u64> = days.iter().map(|d| d.count).collect();
        let sums: Vec<Value> = days.iter().map(|d| number(d.sum)).collect();
        let days_json: Vec<Value> = days
            .iter()
            .zip(runnings)
            .map(|(d, running)| {
                let mut day = json!({"date": d.date, "day": d.day, "weekday": d.weekday, "count": d.count, "sum": number(d.sum)});
                if let (Some(running), Some(obj)) = (running, day.as_object_mut()) {
                    obj.insert("running".into(), running);
                }
                day
            })
            .collect();
        out.insert(
            "month".into(),
            json!({
                "key": key,
                "label": month_label(key),
                "prev": shift_month(key, -1),
                "next": shift_month(key, 1),
                "today": today,
                "days": days_json,
                "labels": labels,
                "counts": counts,
                "sums": sums,
            }),
        );
    }
    if !query.group.is_empty() {
        struct Group<'r, 'a> {
            parts: Vec<Cow<'a, str>>,
            rows: Vec<&'r RecordRowRef<'a>>,
            out: Map<String, Value>,
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut index: FxHashMap<Vec<Cow<str>>, usize> = FxHashMap::default();
        let mut parts: Vec<Cow<str>> = Vec::with_capacity(query.group.len());
        for row in &matched {
            parts.clear();
            parts.extend(query.group.iter().map(|key| group_part(key, row)));
            match index.get(parts.as_slice()) {
                Some(&i) => groups[i].rows.push(row),
                None => {
                    let mut cols = Map::new();
                    for key in &query.group {
                        group_columns(key, row, ctx.names, &mut cols);
                    }
                    index.insert(parts.clone(), groups.len());
                    groups.push(Group { parts: parts.clone(), rows: vec![row], out: cols });
                }
            }
        }
        for g in &mut groups {
            summary(query, &g.rows, &mut g.out);
        }
        let mut columns: Vec<&str> = Vec::new();
        for g in &query.group {
            if g.field == "author" {
                columns.extend(["author", "authorId"]);
            } else {
                columns.push(&g.field);
            }
        }
        columns.push("count");
        if query.sum.is_some() {
            columns.push("sum");
        }
        columns.extend(query.aggregate.iter().map(|a| a.name.as_str()));
        let sort_keys: Vec<&RecordsSortKey> = query.sort.iter().filter(|k| columns.contains(&k.field.as_str())).collect();
        let col = |g: &Group, name: &str| -> Value { g.out.get(name).cloned().unwrap_or(Value::Null) };
        groups.sort_by(|a, b| {
            for key in &sort_keys {
                let (va, vb) = (col(a, &key.field), col(b, &key.field));
                let c = compare(&Field::Json(&va), &Field::Json(&vb));
                if c.is_ne() {
                    return if key.order == RecordsOrder::Asc { c } else { c.reverse() };
                }
            }
            for key in &query.group {
                let (va, vb) = (col(a, &key.field), col(b, &key.field));
                let c = compare(&Field::Json(&va), &Field::Json(&vb));
                if c.is_ne() {
                    return c;
                }
            }
            for (pa, pb) in a.parts.iter().zip(&b.parts) {
                let c = utf16_cmp(pa, pb);
                if c.is_ne() {
                    return c;
                }
            }
            Ordering::Equal
        });
        if !query.running.is_empty() {
            let mut r = seeded();
            for g in &mut groups {
                for row in &g.rows {
                    r.add(row);
                }
                g.out.insert("running".into(), r.read());
            }
        }
        out.insert("groups".into(), Value::Array(groups.into_iter().map(|g| Value::Object(g.out)).collect()));
    }
    if !query.aggregate.is_empty() {
        let mut totals = Map::new();
        summary(query, &matched, &mut totals);
        out.insert("totals".into(), Value::Object(totals));
    }
    if query.before.is_some() {
        let mut b = Map::new();
        summary(query, &before, &mut b);
        out.insert("before".into(), Value::Object(b));
    }
    Some(Value::Object(out))
}

/// [`records_value`] over owned rows (the FFI's).
pub fn records_value_owned(rows: &[RecordRow], query: &RecordsQuery, ctx: RecordsContext) -> Option<Value> {
    let refs: Vec<RecordRowRef> = rows.iter().map(RecordRow::as_ref).collect();
    records_value(&refs, query, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_parses_like_js() {
        assert_eq!(js_string_number(" 12 "), 12.0);
        assert_eq!(js_string_number("0x10"), 16.0);
        assert_eq!(js_string_number("1e2"), 100.0);
        assert_eq!(js_string_number(""), 0.0);
        assert!(js_string_number("12px").is_nan());
        assert!(js_string_number("inf").is_nan());
        assert!(js_string_number("-0x1").is_nan());
        assert_eq!(js_string_number("-Infinity"), f64::NEG_INFINITY);
    }

    #[test]
    fn months_shift_across_years() {
        assert_eq!(shift_month("2026-01", -1), "2025-12");
        assert_eq!(shift_month("2026-12", 1), "2027-01");
        assert_eq!(shift_month("2026-03", -15), "2024-12");
        assert_eq!(month_label("2026-10"), "October 2026");
        assert_eq!(month_days("2028-02").len(), 29);
    }
}

/// `YYYY-MM-DD` digits and dashes (the context's `today`; TS `/^\d{4}-\d{2}-\d{2}$/`).
fn is_day_text(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}
