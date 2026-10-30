//! VAPP-122 — app RECORDS, the Rust twin of
//! `packages/domain-contract/src/app-records.ts` (iOS `AppRecords.swift`,
//! Android `AppRecords.kt`), byte-locked by `fixtures/app-records.json`
//! (`queryCases`, `valueCases`, `countLabelCases`, VAPP-161 `sourceCases`).
//! The `exp:records?…` binding's (and the neutral `records:<kind>?…`) query grammar and the value it emits over the synced
//! `app_records` rows. Pure: the app host feeds it rows, the viewer, the
//! clock and the members' names.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::apps::APPS;

const RECORDS_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/app-records.json");

/// The fixture's `query` block (the grammar's defaults and caps).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsQuerySpec {
    pub reserved: Vec<String>,
    pub sort_default: String,
    pub order_default: String,
    pub limit_default: usize,
    pub limit_max: usize,
    pub operators: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsSpec {
    pub query: RecordsQuerySpec,
    pub row_keys: Vec<String>,
    pub weekdays: Vec<String>,
    pub months: Vec<String>,
}

/// `fixtures/app-records.json` minus its cases.
pub static APP_RECORDS: LazyLock<RecordsSpec> =
    LazyLock::new(|| serde_json::from_str(RECORDS_JSON).expect("app-records.json parses"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordsFilterOp {
    Eq,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl RecordsFilterOp {
    fn wire(self) -> &'static str {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct RecordsFilter {
    pub field: String,
    pub op: RecordsFilterOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordsOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct RecordsQuery {
    pub kind: Option<String>,
    /// Another app's slug; `None` = the hosting app.
    pub app: Option<String>,
    pub mine: bool,
    pub user: Option<String>,
    pub sort: String,
    pub order: RecordsOrder,
    pub limit: usize,
    /// By field, then operator (the fixture's order).
    pub filters: Vec<RecordsFilter>,
    pub sum: Option<String>,
    pub by: Option<String>,
    pub month: Option<String>,
}

/// `YYYY-MM` with a month 01–12.
fn is_month_key(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 7
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && matches!(&text[5..7], "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12")
}

fn trimmed(params: &BTreeMap<String, String>, key: &str) -> Option<String> {
    params.get(key).map(|v| v.trim()).filter(|v| !v.is_empty()).map(str::to_string)
}

/// JS `Number(text)`: whitespace-trimmed, empty = 0, else a decimal float.
fn js_number(text: &str) -> f64 {
    let t = text.trim();
    if t.is_empty() {
        return 0.0;
    }
    // Rust accepts `inf` / `nan` spellings JS does not; neither is finite,
    // so every caller treats them as "not a number" anyway.
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// VAPP-161: a parsed binding source (its scheme, name and params) → the
/// params [`parse_records_query`] takes, for both spellings: the format's
/// neutral `records:<kind>?q` (the name wins over a `kind` param) and
/// `exp:records?kind=<kind>&q`; `None` = not a records source.
pub fn records_source_params(
    scheme: &str,
    name: &str,
    params: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    match (scheme, name) {
        ("records", kind) => {
            let mut out = params.clone();
            out.insert("kind".into(), kind.to_string());
            Some(out)
        }
        ("exp", "records") => Some(params.clone()),
        _ => None,
    }
}

/// The `exp:records` query params (sorted keys, as the source parser gives
/// them) → the query. Unknown keys are data-field filters.
pub fn parse_records_query(params: &BTreeMap<String, String>) -> RecordsQuery {
    let spec = &APP_RECORDS.query;
    let limit = match params.get("limit").map(|v| js_number(v)) {
        Some(n) if n.is_finite() && n.fract() == 0.0 && n > 0.0 => (n.min(spec.limit_max as f64)) as usize,
        _ => spec.limit_default,
    };
    let mut filters = Vec::new();
    for (key, value) in params {
        if spec.reserved.iter().any(|r| r == key) {
            continue;
        }
        let dot = key.rfind('.');
        let op = dot.and_then(|d| RecordsFilterOp::suffix(&key[d + 1..]));
        match (dot, op) {
            (Some(d), Some(op)) if d > 0 => filters.push(RecordsFilter {
                field: key[..d].to_string(),
                op,
                value: value.clone(),
            }),
            _ if !key.is_empty() => filters.push(RecordsFilter {
                field: key.clone(),
                op: RecordsFilterOp::Eq,
                value: value.clone(),
            }),
            _ => {}
        }
    }
    filters.sort_by(|a, b| a.field.cmp(&b.field).then_with(|| a.op.wire().cmp(b.op.wire())));
    let order = match params.get("order").map(String::as_str) {
        Some("asc") => RecordsOrder::Asc,
        Some("desc") => RecordsOrder::Desc,
        _ if spec.order_default == "asc" => RecordsOrder::Asc,
        _ => RecordsOrder::Desc,
    };
    RecordsQuery {
        kind: trimmed(params, "kind"),
        app: trimmed(params, "app"),
        mine: matches!(params.get("mine").map(String::as_str), Some("1") | Some("true")),
        user: trimmed(params, "user"),
        sort: trimmed(params, "sort").unwrap_or_else(|| spec.sort_default.clone()),
        order,
        limit,
        filters,
        sum: trimmed(params, "sum"),
        by: trimmed(params, "by"),
        month: params.get("month").filter(|m| is_month_key(m)).cloned(),
    }
}

/// One synced `app_records` row as the host reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordRowInput<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub data: &'a Value,
    pub created_by: Option<&'a str>,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

/// What [`records_value`] needs besides the rows.
#[derive(Debug, Clone, Copy)]
pub struct RecordsContext<'a> {
    /// The viewer's user id (`None` = signed out: `mine` matches nothing).
    pub me: Option<&'a str>,
    /// ISO 8601 (UTC): the month view's default month.
    pub now: &'a str,
    /// user id → display name (`author`; "" when unknown).
    pub names: &'a HashMap<String, String>,
}

/// A field of the row as a JS value (`undefined` = `None`).
enum Field<'a> {
    Text(&'a str),
    Json(&'a Value),
    Missing,
}

fn num(v: &Field) -> Option<f64> {
    match v {
        Field::Json(Value::Number(n)) => n.as_f64().filter(|f| f.is_finite()),
        Field::Json(Value::String(s)) => num_text(s),
        Field::Text(s) => num_text(s),
        _ => None,
    }
}

fn num_text(s: &str) -> Option<f64> {
    if s.trim().is_empty() {
        return None;
    }
    Some(js_number(s)).filter(|f| f.is_finite())
}

/// JS `String(n)` for the ordinary range.
fn js_number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn text(v: &Field) -> String {
    match v {
        Field::Missing | Field::Json(Value::Null) => String::new(),
        Field::Text(s) => (*s).to_string(),
        Field::Json(Value::String(s)) => s.clone(),
        Field::Json(Value::Bool(b)) => b.to_string(),
        Field::Json(Value::Number(n)) => n.as_f64().map(js_number_text).unwrap_or_else(|| n.to_string()),
        Field::Json(other) => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// `a` against `b` the way a filter and a sort compare: numbers when both
/// parse, else text (code points, no locale: the same ×4).
fn compare(a: &Field, b: &Field) -> std::cmp::Ordering {
    if let (Some(na), Some(nb)) = (num(a), num(b)) {
        return na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal);
    }
    text(a).cmp(&text(b))
}

fn matches(filter: &RecordsFilter, value: &Field) -> bool {
    if filter.op == RecordsFilterOp::Eq {
        return text(value) == filter.value;
    }
    let c = compare(value, &Field::Text(&filter.value));
    match filter.op {
        RecordsFilterOp::Gt => c.is_gt(),
        RecordsFilterOp::Gte => c.is_ge(),
        RecordsFilterOp::Lt => c.is_lt(),
        _ => c.is_le(),
    }
}

fn data_field<'a>(row: &RecordRowInput<'a>, name: &str) -> Field<'a> {
    match row.data.as_object().and_then(|o| o.get(name)) {
        Some(v) => Field::Json(v),
        None => Field::Missing,
    }
}

static NULL: Value = Value::Null;

fn field<'a>(row: &RecordRowInput<'a>, name: &str) -> Field<'a> {
    match name {
        "createdAt" => Field::Text(row.created_at),
        "updatedAt" => Field::Text(row.updated_at),
        "id" => Field::Text(row.id),
        "createdBy" => match row.created_by {
            Some(by) => Field::Text(by),
            None => Field::Json(&NULL),
        },
        _ => data_field(row, name),
    }
}

fn parse_month(key: &str) -> Option<(i64, i64)> {
    let (y, m) = key.split_once('-')?;
    Some((y.parse().ok()?, m.parse().ok()?))
}

/// `YYYY-MM` ± `by` months.
pub fn shift_month(key: &str, by: i64) -> String {
    let (y, m) = parse_month(key).unwrap_or((1970, 1));
    let index = y * 12 + (m - 1) + by;
    format!("{}-{:02}", index.div_euclid(12), index.rem_euclid(12) + 1)
}

/// `October 2026`.
pub fn month_label(key: &str) -> String {
    let (y, m) = parse_month(key).unwrap_or((1970, 1));
    let name = APP_RECORDS.months.get((m - 1).clamp(0, 11) as usize).cloned().unwrap_or_default();
    format!("{name} {y}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
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
    pub weekday: String,
    pub count: u64,
    pub sum: f64,
}

/// Every day of `key` (`YYYY-MM`), empty.
pub fn month_days(key: &str) -> Vec<RecordsDay> {
    let (y, m) = parse_month(key).unwrap_or((1970, 1));
    let m = m.clamp(1, 12);
    (1..=days_in_month(y, m))
        .map(|day| RecordsDay {
            date: format!("{key}-{day:02}"),
            day,
            weekday: APP_RECORDS.weekdays[weekday(y, m, day)].clone(),
            count: 0,
            sum: 0.0,
        })
        .collect()
}

/// JS `Math.round(n * 1e6) / 1e6`: sums of hours like 0.1 + 0.2 stay
/// readable (and identical ×4).
fn round(n: f64) -> f64 {
    (n * 1_000_000.0 + 0.5).floor() / 1_000_000.0
}

/// A JS number as JSON: integral values as integers (so `0` equals `0`).
fn number_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

/// The value `exp:records` emits for `query` over `rows` (every synced row
/// of the app, any kind): `None` without a kind; else {rows, count, sum?,
/// month?} as the fixture spells out.
pub fn records_value(rows: &[RecordRowInput], query: &RecordsQuery, ctx: RecordsContext) -> Option<Value> {
    let kind = query.kind.as_deref()?;
    let month_key: Option<String> = query.by.as_ref().map(|_| {
        query
            .month
            .clone()
            .unwrap_or_else(|| ctx.now.chars().take(7).collect())
    });
    let mut matched: Vec<&RecordRowInput> = rows
        .iter()
        .filter(|row| {
            if row.kind != kind {
                return false;
            }
            if query.mine && (ctx.me.is_none() || row.created_by != ctx.me) {
                return false;
            }
            if let Some(user) = &query.user {
                if row.created_by != Some(user.as_str()) {
                    return false;
                }
            }
            if let (Some(key), Some(by)) = (&month_key, &query.by) {
                if !text(&data_field(row, by)).starts_with(key.as_str()) {
                    return false;
                }
            }
            query.filters.iter().all(|f| matches(f, &field(row, &f.field)))
        })
        .collect();
    matched.sort_by(|a, b| {
        let c = compare(&field(a, &query.sort), &field(b, &query.sort));
        if c.is_ne() {
            return if query.order == RecordsOrder::Asc { c } else { c.reverse() };
        }
        a.id.cmp(b.id)
    });
    let out_rows: Vec<Value> = matched
        .iter()
        .take(query.limit)
        .map(|row| {
            let mut obj: Map<String, Value> = row.data.as_object().cloned().unwrap_or_default();
            obj.insert("id".into(), json!(row.id));
            obj.insert("createdAt".into(), json!(row.created_at));
            obj.insert("updatedAt".into(), json!(row.updated_at));
            obj.insert("createdBy".into(), json!(row.created_by));
            let author = row
                .created_by
                .and_then(|by| ctx.names.get(by))
                .filter(|n| !n.is_empty())
                .cloned()
                .unwrap_or_default();
            obj.insert("author".into(), json!(author));
            Value::Object(obj)
        })
        .collect();
    let mut out = Map::new();
    out.insert("rows".into(), Value::Array(out_rows));
    out.insert("count".into(), json!(matched.len()));
    if let Some(sum) = &query.sum {
        let total = matched.iter().fold(0.0, |acc, row| acc + num(&data_field(row, sum)).unwrap_or(0.0));
        out.insert("sum".into(), number_value(round(total)));
    }
    if let (Some(key), Some(by)) = (&month_key, &query.by) {
        let mut days = month_days(key);
        for row in &matched {
            let date: String = text(&data_field(row, by)).chars().take(10).collect();
            let Some(day) = days.iter_mut().find(|d| d.date == date) else {
                continue;
            };
            day.count += 1;
            if let Some(sum) = &query.sum {
                day.sum = round(day.sum + num(&data_field(row, sum)).unwrap_or(0.0));
            }
        }
        let labels: Vec<String> = days.iter().map(|d| d.day.to_string()).collect();
        let counts: Vec<u64> = days.iter().map(|d| d.count).collect();
        let sums: Vec<Value> = days.iter().map(|d| number_value(d.sum)).collect();
        let today: String = ctx.now.chars().take(10).collect();
        let days: Vec<Value> = days
            .into_iter()
            .map(|d| {
                json!({
                    "date": d.date,
                    "day": d.day,
                    "weekday": d.weekday,
                    "count": d.count,
                    "sum": number_value(d.sum),
                })
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
                "days": days,
                "labels": labels,
                "counts": counts,
                "sums": sums,
            }),
        );
    }
    Some(Value::Object(out))
}

/// en-US grouping: `1,240`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Settings → Apps → the app: "1,240 records", "1,240 of 25,000 records"
/// (`max` = the team's quota, `None` = unlimited), "No records".
pub fn records_count_label(count: u64, max: Option<u64>) -> String {
    let copy = &APPS.copy;
    if count == 0 {
        return copy.records_none.clone();
    }
    let n = grouped(count);
    match max {
        None if count == 1 => copy.records_one.clone(),
        None => copy.records.replace("{n}", &n),
        Some(max) => copy.records_of.replace("{n}", &n).replace("{max}", &grouped(max)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(RECORDS_JSON).unwrap();
        fixture[key].as_array().unwrap().clone()
    }

    fn params(v: &Value) -> BTreeMap<String, String> {
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect()
    }

    #[test]
    fn the_fixture_parses() {
        assert_eq!(APP_RECORDS.query.limit_max, 500);
        assert_eq!(APP_RECORDS.weekdays.len(), 7);
        assert_eq!(APP_RECORDS.months.len(), 12);
        assert_eq!(shift_month("2026-01", -1), "2025-12");
        assert_eq!(shift_month("2026-12", 1), "2027-01");
        assert_eq!(month_label("2026-10"), "October 2026");
        assert_eq!(month_days("2024-02").len(), 29);
    }

    #[test]
    fn query_cases_replay_the_fixture() {
        for case in cases("queryCases") {
            let name = case["name"].as_str().unwrap();
            let got = parse_records_query(&params(&case["params"]));
            let want: RecordsQuery = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(got, want, "{name}");
        }
    }

    /// VAPP-161: parse the source, map it, parse the query.
    #[test]
    fn source_cases_replay_the_fixture() {
        let all = cases("sourceCases");
        assert!(!all.is_empty());
        for case in all {
            let name = case["name"].as_str().unwrap();
            let source = eui_core::host::parse_source(case["source"].as_str().unwrap()).unwrap();
            let got = records_source_params(&source.scheme, &source.name, &source.params).map(|p| parse_records_query(&p));
            let want: Option<RecordsQuery> = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(got, want, "{name}");
        }
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureRow {
        id: String,
        kind: String,
        data: Value,
        created_by: Option<String>,
        created_at: String,
        updated_at: String,
    }

    #[test]
    fn value_cases_replay_the_fixture() {
        for case in cases("valueCases") {
            let name = case["name"].as_str().unwrap();
            let rows: Vec<FixtureRow> = serde_json::from_value(case["rows"].clone()).unwrap();
            let inputs: Vec<RecordRowInput> = rows
                .iter()
                .map(|r| RecordRowInput {
                    id: &r.id,
                    kind: &r.kind,
                    data: &r.data,
                    created_by: r.created_by.as_deref(),
                    created_at: &r.created_at,
                    updated_at: &r.updated_at,
                })
                .collect();
            let names: HashMap<String, String> = serde_json::from_value(case["names"].clone()).unwrap();
            let query = parse_records_query(&params(&case["params"]));
            let ctx = RecordsContext {
                me: case["me"].as_str(),
                now: case["now"].as_str().unwrap(),
                names: &names,
            };
            let got = records_value(&inputs, &query, ctx).unwrap_or(Value::Null);
            assert_eq!(got, case["expected"], "{name}");
        }
    }

    #[test]
    fn count_label_cases_replay_the_fixture() {
        for case in cases("countLabelCases") {
            let label = records_count_label(case["count"].as_u64().unwrap(), case["max"].as_u64());
            assert_eq!(label, case["label"].as_str().unwrap());
        }
    }

    #[test]
    fn sums_round_like_js() {
        assert_eq!(round(0.1 + 0.2), 0.3);
        assert_eq!(number_value(round(3.0 + 0.5)), json!(3.5));
        assert_eq!(number_value(0.0), json!(0));
    }
}
