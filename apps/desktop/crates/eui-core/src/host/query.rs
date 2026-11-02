//! VAPP-189 (S1): THE records query, the Rust twin of
//! `packages/exponential-ui/src/host/query.ts`, locked by
//! `fixtures/records-query.json` (`parseCases`, `sourceCases`,
//! `valueCases`). A binding's URL sugar (`records:<kind>?q`, a host's own
//! spelling like Exponential's `exp:records?kind=<kind>&q`) parses INTO the
//! structured [`RecordsQuery`]; [`records_value`] evaluates it over the rows
//! of any store. The desktop host calls it directly; the Swift and Compose
//! SDKs reach it through `exponential-ui-ffi` (`parse_records_query_json`,
//! `parse_records_source_json`, `records_value_json`).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::sources::ParsedSource;
use super::{js_trim, utf16_cmp};
use crate::json::{canonical, number, number_to_string};

/// The binding scheme: `records:<kind>[?query]`.
pub const RECORDS_SCHEME: &str = "records";
/// The URL sugar's defaults and caps (catalog/host.json `package.records.query`).
pub const RECORDS_SORT_DEFAULT: &str = "createdAt";
pub const RECORDS_ORDER_DEFAULT: &str = "desc";
pub const RECORDS_LIMIT_DEFAULT: u32 = 100;
pub const RECORDS_LIMIT_MAX: u32 = 500;
/// Query keys with a meaning of their own; never a field filter.
pub const RECORDS_RESERVED_PARAMS: &[&str] = &["app", "by", "kind", "limit", "mine", "month", "order", "sort", "sum", "user"];
/// The keys every value row carries next to its data (a data key never shadows one).
pub const RECORDS_ROW_KEYS: &[&str] = &["id", "createdAt", "updatedAt", "createdBy", "authorId", "author"];
/// The filter operators: `eq` is the bare key, the others a `.<op>` suffix.
pub const RECORDS_FILTER_OPS: &[&str] = &["eq", "gt", "gte", "lt", "lte"];
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
}

impl RecordsFilterOp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
        }
    }

    /// The suffix operators (`<field>.<op>`); `eq` is the bare key.
    fn suffix(text: &str) -> Option<Self> {
        match text {
            "gt" => Some(Self::Gt),
            "gte" => Some(Self::Gte),
            "lt" => Some(Self::Lt),
            "lte" => Some(Self::Lte),
            _ => None,
        }
    }
}

/// One condition: `eq` compares as text, the ranges as numbers when both
/// sides parse, else as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordsFilter {
    pub field: String,
    pub op: RecordsFilterOp,
    pub value: String,
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

/// The structured query (the canonical form's subset; a JSON query form adds
/// keys next to these).
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
    /// Every condition holds; by field, then operator.
    #[serde(rename = "where")]
    pub where_: Vec<RecordsFilter>,
    /// Sort keys in order; ties always fall back to the id.
    pub sort: Vec<RecordsSortKey>,
    pub limit: u32,
    /// Σ of this field over the matches (before the limit).
    pub sum: Option<String>,
    pub month: Option<RecordsMonthView>,
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
/// = the source's own name (the neutral `records:<kind>`), which wins over a
/// `kind` param. Unknown keys are field filters.
pub fn parse_records_query(params: &BTreeMap<String, String>, kind: Option<&str>) -> RecordsQuery {
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
            Some((dot, op)) if dot > 0 => where_.push(RecordsFilter { field: key[..dot].to_string(), op, value }),
            _ => where_.push(RecordsFilter { field: key.clone(), op: RecordsFilterOp::Eq, value }),
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
    RecordsQuery {
        from: trimmed(kind.or(params.get("kind").map(String::as_str))),
        app: trimmed(params.get("app").map(String::as_str)),
        mine: matches!(params.get("mine").map(String::as_str), Some("1" | "true")),
        user: trimmed(params.get("user").map(String::as_str)),
        where_,
        sort: vec![RecordsSortKey { field: trimmed(params.get("sort").map(String::as_str)).unwrap_or_else(|| RECORDS_SORT_DEFAULT.to_string()), order }],
        limit,
        sum: trimmed(params.get("sum").map(String::as_str)),
        month: by.map(|field| RecordsMonthView { field, key: params.get("month").filter(|m| is_month_key(m)).cloned() }),
    }
}

/// `records:<kind>?…` → the query; `None` = not a records source.
pub fn parse_records_source(source: &ParsedSource) -> Option<RecordsQuery> {
    (source.scheme == RECORDS_SCHEME).then(|| parse_records_query(&source.params, Some(&source.name)))
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
    /// ISO 8601 (UTC): the month view's default month and `today`.
    pub now: &'a str,
    /// user id → display name (`author`; "" when unknown).
    pub names: &'a HashMap<String, String>,
}

/// A field as a JS value (`undefined` = `Missing`).
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

fn text(v: &Field) -> String {
    match v {
        Field::Missing | Field::Json(Value::Null) => String::new(),
        Field::Text(s) => (*s).to_string(),
        Field::Json(Value::String(s)) => s.clone(),
        Field::Json(Value::Bool(b)) => b.to_string(),
        Field::Json(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(0.0)),
        Field::Json(other) => serde_json::to_string(&canonical(other)).unwrap_or_default(),
    }
}

/// Numbers when both parse, else text (UTF-16 code units, no locale).
fn compare(a: &Field, b: &Field) -> Ordering {
    if let (Some(na), Some(nb)) = (num(a), num(b)) {
        return na.partial_cmp(&nb).unwrap_or(Ordering::Equal);
    }
    utf16_cmp(&text(a), &text(b))
}

fn holds(filter: &RecordsFilter, value: &Field) -> bool {
    if filter.op == RecordsFilterOp::Eq {
        return text(value) == filter.value;
    }
    let c = compare(value, &Field::Text(&filter.value));
    match filter.op {
        RecordsFilterOp::Gt => c.is_gt(),
        RecordsFilterOp::Gte => c.is_ge(),
        RecordsFilterOp::Lt => c.is_lt(),
        RecordsFilterOp::Lte | RecordsFilterOp::Eq => c.is_le(),
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

fn days_in_month(y: i64, m: i64) -> i64 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// 0 = Sunday (Sakamoto's method, proleptic Gregorian).
fn weekday(y: i64, m: i64, d: i64) -> usize {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[(m - 1) as usize] + d).rem_euclid(7) as usize
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
    (1..=days_in_month(y, m)).map(|day| RecordsDay { date: format!("{key}-{day:02}"), day, weekday: RECORDS_WEEKDAYS[weekday(y, m, day)], count: 0, sum: 0.0 }).collect()
}

/// JS `Math.round(n * 1e6) / 1e6`: sums like 0.1 + 0.2 stay readable (and identical ×4).
fn round(n: f64) -> f64 {
    (n * 1_000_000.0 + 0.5).floor() / 1_000_000.0
}

/// `s.slice(0, n)` (dates and ISO timestamps: ASCII).
fn prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// The value a records binding emits for `query` over `rows` (every row of
/// the app, any kind): `None` without a kind; else {rows, count, sum?,
/// month?}: the matches sorted (ties by id) then cut at the limit, `count`
/// and `sum` before the limit.
pub fn records_value(rows: &[RecordRowRef], query: &RecordsQuery, ctx: RecordsContext) -> Option<Value> {
    let kind = query.from.as_deref()?;
    let view = query.month.as_ref();
    let month_key: Option<String> = view.map(|v| v.key.clone().unwrap_or_else(|| prefix(ctx.now, 7)));
    let mut matched: Vec<&RecordRowRef> = rows
        .iter()
        .filter(|row| {
            if row.kind != kind {
                return false;
            }
            let author = row.author();
            if query.mine && (ctx.me.is_none() || author != ctx.me) {
                return false;
            }
            if let Some(user) = &query.user {
                if author != Some(user.as_str()) {
                    return false;
                }
            }
            if let (Some(view), Some(key)) = (view, &month_key) {
                if !text(&data_field(row, &view.field)).starts_with(key.as_str()) {
                    return false;
                }
            }
            query.where_.iter().all(|f| holds(f, &field(row, &f.field)))
        })
        .collect();
    matched.sort_by(|a, b| {
        for key in &query.sort {
            let c = compare(&field(a, &key.field), &field(b, &key.field));
            if c.is_ne() {
                return if key.order == RecordsOrder::Asc { c } else { c.reverse() };
            }
        }
        utf16_cmp(a.id, b.id)
    });
    let out_rows: Vec<Value> = matched
        .iter()
        .take(query.limit as usize)
        .map(|row| {
            let mut obj: Map<String, Value> = row.data.as_object().cloned().unwrap_or_default();
            obj.insert("id".into(), json!(row.id));
            obj.insert("createdAt".into(), json!(row.created_at));
            obj.insert("updatedAt".into(), json!(row.updated_at));
            obj.insert("createdBy".into(), json!(row.created_by));
            obj.insert("authorId".into(), json!(row.author()));
            let author = row.author().and_then(|a| ctx.names.get(a)).cloned().unwrap_or_default();
            obj.insert("author".into(), json!(author));
            Value::Object(obj)
        })
        .collect();
    let mut out = Map::new();
    out.insert("rows".into(), Value::Array(out_rows));
    out.insert("count".into(), json!(matched.len()));
    let sum_of = |row: &RecordRowRef, f: &str| num_value(row.data.as_object().and_then(|o| o.get(f))).unwrap_or(0.0);
    if let Some(sum) = &query.sum {
        let total = matched.iter().fold(0.0, |acc, row| acc + sum_of(row, sum));
        out.insert("sum".into(), number(round(total)));
    }
    if let (Some(view), Some(key)) = (view, &month_key) {
        let mut days = month_days(key);
        for row in &matched {
            let date = prefix(&text(&data_field(row, &view.field)), 10);
            let Some(day) = days.iter_mut().find(|d| d.date == date) else {
                continue;
            };
            day.count += 1;
            if let Some(sum) = &query.sum {
                day.sum = round(day.sum + sum_of(row, sum));
            }
        }
        let labels: Vec<String> = days.iter().map(|d| d.day.to_string()).collect();
        let counts: Vec<u64> = days.iter().map(|d| d.count).collect();
        let sums: Vec<Value> = days.iter().map(|d| number(d.sum)).collect();
        let days_json: Vec<Value> = days.iter().map(|d| json!({"date": d.date, "day": d.day, "weekday": d.weekday, "count": d.count, "sum": number(d.sum)})).collect();
        out.insert(
            "month".into(),
            json!({
                "key": key,
                "label": month_label(key),
                "prev": shift_month(key, -1),
                "next": shift_month(key, 1),
                "today": prefix(ctx.now, 10),
                "days": days_json,
                "labels": labels,
                "counts": counts,
                "sums": sums,
            }),
        );
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
