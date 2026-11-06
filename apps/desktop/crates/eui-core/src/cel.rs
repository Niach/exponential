//! VAPP-191 (eApps Q2): expressions, the Rust twin of
//! `packages/exponential-ui/src/cel.ts` (line by line; read its header for
//! the language). A SUBSET of CEL with ONE number type (a double, like
//! JSON), hand-written so both twins replay `fixtures/expressions.json`
//! exactly: numbers, dates and error texts. No loops, no user functions, a
//! length cap, no nested comprehensions and a step limit, so it always
//! terminates. The functions are ONE table shared with the A2UI function
//! form (`{"call": "mul", …}`, [`call_expression_function`]); dates run in
//! the team zone (an offset per instant, [`ExpressionEnv::offset`]).

use crate::dates::{day_from_number, day_number, days_in_month, format_day, iso_week, parse_day, weekday_of};
use crate::json;
use serde_json::{Map, Value};

/// The cost caps (spec §4.2): characters (code points), nesting, steps.
pub const MAX_LENGTH: usize = 1000;
pub const MAX_DEPTH: usize = 32;
pub const MAX_STEPS: usize = 10_000;

/// Every unit `dateAdd` / `dateDiff` take; `startOf` / `endOf` take the calendar ones.
pub const DATE_UNITS: &[&str] = &["second", "minute", "hour", "day", "week", "month", "quarter", "year"];
pub const CALENDAR_UNITS: &[&str] = &["day", "week", "month", "quarter", "year"];
/// The formats `formatDuration` writes.
pub const DURATION_FORMATS: &[&str] = &["h:mm", "hh:mm"];

/// VAPP-194 (E1): what calling a function does. Expressions are PURE: a
/// binding, `visible`, a computed field or a query param never writes. A
/// `write` changes records or the surface data, a `host` function reaches
/// outside the surface (a toast, an MCP call, a link, another action); both
/// run only as steps of a package action ([`crate::host::actions`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionEffect {
    Pure,
    Write,
    Host,
}

impl FunctionEffect {
    /// `pure` | `write` | `host`.
    pub fn as_str(self) -> &'static str {
        match self {
            FunctionEffect::Pure => "pure",
            FunctionEffect::Write => "write",
            FunctionEffect::Host => "host",
        }
    }
}

/// The functions that write (records, the surface data model).
pub const WRITE_FUNCTIONS: &[&str] = &["set", "records.upsert", "records.delete", "records.batch"];
/// The built-in host functions; every name under a host prefix is one too.
pub const HOST_FUNCTIONS: &[&str] = &["openUrl", "bind", "app.navigate"];
// VAPP-271: every Engine capability (catalog/host.json `package.capabilities.scopes` names) is a host function too.
pub const HOST_FUNCTION_PREFIXES: &[&str] = &["actions.", "harness.", "exponential.", "tools.", "upstream.", "camera.", "clipboard.", "files.", "http.", "location.", "notifications.", "photos.", "print.", "share."];

/// The effect of a function by name: the signature table's (every
/// expression function, every bind-time function) is `pure`.
pub fn function_effect(name: &str) -> FunctionEffect {
    if WRITE_FUNCTIONS.contains(&name) {
        return FunctionEffect::Write;
    }
    if HOST_FUNCTIONS.contains(&name) || HOST_FUNCTION_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return FunctionEffect::Host;
    }
    FunctionEffect::Pure
}

/// Why an effectful function is refused in a pure place.
pub fn effect_words(name: &str, effect: FunctionEffect) -> String {
    format!("{name} {}", if effect == FunctionEffect::Write { "writes" } else { "is a host function" })
}

/// The clock, the team zone and the step limit an evaluation runs with.
#[derive(Clone, Copy, Default)]
pub struct ExpressionEnv<'a> {
    /// Epoch ms `today()` / `now()` read; `None` = the wall clock.
    pub now: Option<f64>,
    /// Minutes east of UTC at an instant; `None` = UTC.
    pub offset: Option<&'a (dyn Fn(f64) -> i32 + Send + Sync)>,
    /// The step limit (default [`MAX_STEPS`]).
    pub max_steps: Option<usize>,
}

impl std::fmt::Debug for ExpressionEnv<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpressionEnv").field("now", &self.now).field("max_steps", &self.max_steps).finish_non_exhaustive()
    }
}

struct Env<'a> {
    now: f64,
    offset: Option<&'a (dyn Fn(f64) -> i32 + Send + Sync)>,
}

impl<'a> Env<'a> {
    fn of(env: &ExpressionEnv<'a>) -> Env<'a> {
        Env { now: env.now.unwrap_or_else(crate::format::now_ms).floor(), offset: env.offset }
    }
    fn off(&self, ms: f64) -> f64 {
        self.offset.map_or(0.0, |o| f64::from(o(ms)))
    }
}

type R<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// The runtime type name errors print.
pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Bool(_) => "bool",
        Value::Array(_) => "list",
        Value::Object(_) => "map",
    }
}

/// A finite result with `-0` as 0; anything else is out of range.
fn num(v: f64) -> R<f64> {
    if !v.is_finite() {
        return Err("number out of range".into());
    }
    Ok(if v == 0.0 { 0.0 } else { v })
}

fn num_text(n: f64) -> String {
    json::number_to_string(n)
}

fn deep_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| deep_equal(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| deep_equal(v, w))),
        _ => a == b,
    }
}

/// Round half away from zero without `x + 0.5`.
fn round_half_away(x: f64) -> f64 {
    let a = x.abs();
    let f = a.floor();
    let r = if a - f >= 0.5 { f + 1.0 } else { f };
    if x < 0.0 {
        -r
    } else {
        r
    }
}

fn utf16_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

// ---------------------------------------------------------------------------
// Dates (the team zone)
// ---------------------------------------------------------------------------

const DAY_MS: f64 = 86_400_000.0;

fn unit_ms(unit: &str) -> Option<f64> {
    match unit {
        "second" => Some(1000.0),
        "minute" => Some(60_000.0),
        "hour" => Some(3_600_000.0),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Temporal {
    Date(i64),
    Ts(f64),
}

/// An ISO instant (`YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM]`, no offset = UTC)
/// as epoch ms; `None` when it is not one.
pub fn parse_timestamp(text: &str) -> Option<f64> {
    let b = text.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let int = |from: usize, n: usize| b[from..from + n].iter().fold(0i64, |acc, c| acc * 10 + i64::from(c - b'0'));
    if b.len() < 16 || b[10] != b'T' || !(digit(11) && digit(12) && b[13] == b':' && digit(14) && digit(15)) {
        return None;
    }
    let day = parse_day(&text[..10]).filter(|_| text.is_char_boundary(10))?;
    if !(b[..10].iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })) {
        return None;
    }
    let (h, mi) = (int(11, 2), int(14, 2));
    let mut i = 16;
    let mut s = 0;
    let mut ms = 0;
    if b.get(i) == Some(&b':') {
        if !(digit(i + 1) && digit(i + 2)) {
            return None;
        }
        s = int(i + 1, 2);
        i += 3;
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            let mut end = start;
            while digit(end) {
                end += 1;
            }
            if end == start {
                return None;
            }
            let frac: String = text[start..end.min(start + 3)].chars().collect();
            ms = format!("{frac:0<3}").parse::<i64>().ok()?;
            i = end;
        }
    }
    let mut offset = 0;
    match b.get(i) {
        None => {}
        Some(b'Z') if i + 1 == b.len() => {}
        Some(c @ (b'+' | b'-')) if b.len() == i + 6 && digit(i + 1) && digit(i + 2) && b[i + 3] == b':' && digit(i + 4) && digit(i + 5) => {
            let (oh, om) = (int(i + 1, 2), int(i + 4, 2));
            if oh > 23 || om > 59 {
                return None;
            }
            offset = (if *c == b'-' { -1 } else { 1 }) * (oh * 60 + om);
        }
        _ => return None,
    }
    if h > 23 || mi > 59 || s > 59 {
        return None;
    }
    Some((day_number(day.y, day.m, day.d) as f64) * DAY_MS + (h * 3_600_000 + mi * 60_000 + s * 1000 + ms - offset * 60_000) as f64)
}

/// An instant as `YYYY-MM-DDTHH:MM:SS.sssZ` (JS `toISOString`), years 0..9999.
pub fn format_timestamp(ms: f64) -> R<String> {
    if !(ms >= MIN_DAY as f64 * DAY_MS && ms < (MAX_DAY + 1) as f64 * DAY_MS) {
        return Err("date out of range".into());
    }
    let day = (ms / DAY_MS).floor();
    let tod = (ms - day * DAY_MS) as i64;
    let d = day_from_number(day as i64);
    if d.y < 0 || d.y > 9999 {
        return Err("date out of range".into());
    }
    Ok(format!("{}T{:02}:{:02}:{:02}.{:03}Z", format_day(d), tod / 3_600_000, (tod / 60_000) % 60, (tod / 1000) % 60, tod % 1000))
}

/// The days a date may be: 0000-01-01 ..= 9999-12-31 (checked BEFORE the
/// calendar maths, so a huge day never overflows it).
const MIN_DAY: i64 = -719_528;
const MAX_DAY: i64 = 2_932_896;
/// The most calendar steps (days) any `dateAdd` can take and stay in range.
const MAX_CALENDAR_STEPS: f64 = 4_000_000.0;

fn day_text(day: i64) -> R<String> {
    if !(MIN_DAY..=MAX_DAY).contains(&day) {
        return Err("date out of range".into());
    }
    let d = day_from_number(day);
    if d.y < 0 || d.y > 9999 {
        return Err("date out of range".into());
    }
    Ok(format_day(d))
}

/// The wall clock (as if UTC) of an instant in the zone.
fn to_local(env: &Env, ms: f64) -> f64 {
    ms + env.off(ms) * 60_000.0
}

/// The instant of a wall-clock time in the zone: a time in a DST gap moves
/// forward by the gap; a repeated time takes its later (standard) instant.
fn from_local(env: &Env, wall: f64) -> f64 {
    let first = env.off(wall);
    let utc = wall - first * 60_000.0;
    let second = env.off(utc);
    if second == first {
        utc
    } else {
        wall - second * 60_000.0
    }
}

fn temporal(f: &str, v: &Value) -> R<Temporal> {
    let Value::String(s) = v else { return Err(format!("{f} needs a date, got {}", type_name(v))) };
    if s.chars().count() == 10 {
        if let Some(d) = parse_day(s) {
            return Ok(Temporal::Date(day_number(d.y, d.m, d.d)));
        }
    }
    parse_timestamp(s).map(Temporal::Ts).ok_or_else(|| format!("not a date: {s}"))
}

fn floor_day(wall: f64) -> i64 {
    (wall / DAY_MS).floor() as i64
}

fn wall_of(env: &Env, t: Temporal) -> f64 {
    match t {
        Temporal::Date(day) => day as f64 * DAY_MS,
        Temporal::Ts(ms) => to_local(env, ms),
    }
}

fn unit_of<'a>(f: &str, v: &'a Value, units: &[&str]) -> R<&'a str> {
    let Value::String(s) = v else { return Err(format!("{f} needs a string, got {}", type_name(v))) };
    if !units.contains(&s.as_str()) {
        return Err(format!("unknown unit {s}"));
    }
    Ok(s)
}

fn add_months(day: i64, months: i64) -> i64 {
    let c = day_from_number(day);
    let total = c.y * 12 + (c.m - 1) + months;
    let y = total.div_euclid(12);
    let m = total - y * 12 + 1;
    day_number(y, m, c.d.min(days_in_month(y, m)))
}

fn add_calendar(day: i64, n: i64, unit: &str) -> i64 {
    match unit {
        "day" => day + n,
        "week" => day + 7 * n,
        "month" => add_months(day, n),
        "quarter" => add_months(day, n * 3),
        _ => add_months(day, n * 12),
    }
}

fn start_day(day: i64, unit: &str) -> i64 {
    let c = day_from_number(day);
    match unit {
        "day" => day,
        "week" => {
            let wd = match weekday_of(c.y, c.m, c.d) {
                0 => 7,
                n => n as i64,
            };
            day - (wd - 1)
        }
        "month" => day_number(c.y, c.m, 1),
        "quarter" => day_number(c.y, (c.m - 1) / 3 * 3 + 1, 1),
        _ => day_number(c.y, 1, 1),
    }
}

fn number(f: &str, v: &Value) -> R<f64> {
    match v {
        Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        _ => Err(format!("{f} needs a number, got {}", type_name(v))),
    }
}

fn whole_number(f: &str, v: &Value) -> R<f64> {
    let n = number(f, v)?;
    if n.fract() != 0.0 {
        return Err(format!("{f} needs a whole number, got {}", num_text(n)));
    }
    Ok(n)
}

fn text<'a>(f: &str, v: &'a Value) -> R<&'a str> {
    match v {
        Value::String(s) => Ok(s),
        _ => Err(format!("{f} needs a string, got {}", type_name(v))),
    }
}

/// VAPP-245: a day and a wall-clock time (`HH:MM`, none = midnight) in the
/// team zone as an instant: the Record form's date-time pair.
fn date_time(args: &[Value], env: &Env) -> R<Value> {
    let Temporal::Date(day) = temporal("dateTime", &args[0])? else {
        return Err("dateTime needs a day".into());
    };
    let clock = match args.get(1) {
        None | Some(Value::Null) => "00:00",
        Some(v) => text("dateTime", v)?,
    };
    let bytes = clock.as_bytes();
    let digits = |i: usize| bytes[i].is_ascii_digit();
    if bytes.len() != 5 || bytes[2] != b':' || !digits(0) || !digits(1) || !digits(3) || !digits(4) {
        return Err(format!("not a time: {clock}"));
    }
    let (h, m) = ((bytes[0] - b'0') as i64 * 10 + (bytes[1] - b'0') as i64, (bytes[3] - b'0') as i64 * 10 + (bytes[4] - b'0') as i64);
    if h > 23 || m > 59 {
        return Err(format!("not a time: {clock}"));
    }
    Ok(Value::String(format_timestamp(from_local(env, day as f64 * DAY_MS + ((h * 60 + m) * 60_000) as f64))?))
}

fn date_add(args: &[Value], env: &Env) -> R<Value> {
    let t = temporal("dateAdd", &args[0])?;
    let n = whole_number("dateAdd", &args[1])?;
    let unit = unit_of("dateAdd", &args[2], DATE_UNITS)?;
    let out = match t {
        Temporal::Date(day) => {
            if unit_ms(unit).is_some() {
                return Err(format!("unit {unit} needs a timestamp"));
            }
            if !(n.abs() <= MAX_CALENDAR_STEPS) {
                return Err("date out of range".into());
            }
            day_text(add_calendar(day, n as i64, unit))?
        }
        Temporal::Ts(ms) => match unit_ms(unit) {
            Some(u) => format_timestamp(num(ms + n * u)?)?,
            None => {
                if !(n.abs() <= MAX_CALENDAR_STEPS) {
                    return Err("date out of range".into());
                }
                let wall = to_local(env, ms);
                let day = floor_day(wall);
                let tod = wall - day as f64 * DAY_MS;
                format_timestamp(from_local(env, add_calendar(day, n as i64, unit) as f64 * DAY_MS + tod))?
            }
        },
    };
    Ok(Value::String(out))
}

fn date_diff(args: &[Value], env: &Env) -> R<Value> {
    let a = temporal("dateDiff", &args[0])?;
    let b = temporal("dateDiff", &args[1])?;
    let unit = unit_of("dateDiff", &args[2], DATE_UNITS)?;
    if let Some(u) = unit_ms(unit) {
        let instant = |t: Temporal| match t {
            Temporal::Date(day) => from_local(env, day as f64 * DAY_MS),
            Temporal::Ts(ms) => ms,
        };
        return Ok(json::number(num(((instant(b) - instant(a)) / u).trunc())?));
    }
    let (wa, wb) = (wall_of(env, a), wall_of(env, b));
    if unit == "day" || unit == "week" {
        let span = if unit == "day" { DAY_MS } else { 7.0 * DAY_MS };
        return Ok(json::number(num(((wb - wa) / span).trunc())?));
    }
    let (da, db) = (floor_day(wa), floor_day(wb));
    let (ca, cb) = (day_from_number(da), day_from_number(db));
    let mut months = (cb.y - ca.y) * 12 + (cb.m - ca.m);
    let rest_a = ca.d as f64 * DAY_MS + (wa - da as f64 * DAY_MS);
    let rest_b = cb.d as f64 * DAY_MS + (wb - db as f64 * DAY_MS);
    if months > 0 && rest_b < rest_a {
        months -= 1;
    } else if months < 0 && rest_b > rest_a {
        months += 1;
    }
    let per = match unit {
        "month" => 1.0,
        "quarter" => 3.0,
        _ => 12.0,
    };
    Ok(json::number(num((months as f64 / per).trunc())?))
}

fn start_end(f: &str, args: &[Value], env: &Env) -> R<Value> {
    let t = temporal(f, &args[0])?;
    let unit = unit_of(f, &args[1], CALENDAR_UNITS)?;
    let day = match t {
        Temporal::Date(day) => day,
        Temporal::Ts(ms) => floor_day(to_local(env, ms)),
    };
    let is_date = matches!(t, Temporal::Date(_));
    let out = if f == "startOf" {
        let start = start_day(day, unit);
        if is_date {
            day_text(start)?
        } else {
            format_timestamp(from_local(env, start as f64 * DAY_MS))?
        }
    } else {
        let next = add_calendar(start_day(day, unit), 1, unit);
        if is_date {
            day_text(next - 1)?
        } else {
            format_timestamp(from_local(env, next as f64 * DAY_MS) - 1.0)?
        }
    };
    Ok(Value::String(out))
}

fn local_day(f: &str, v: &Value, env: &Env) -> R<crate::dates::CalendarDay> {
    Ok(day_from_number(match temporal(f, v)? {
        Temporal::Date(day) => day,
        Temporal::Ts(ms) => floor_day(to_local(env, ms)),
    }))
}

fn format_duration(args: &[Value]) -> R<Value> {
    let h = number("formatDuration", &args[0])?;
    let format = text("formatDuration", &args[1])?;
    if !DURATION_FORMATS.contains(&format) {
        return Err(format!("unknown duration format {format}"));
    }
    let total = round_half_away(h.abs() * 60.0);
    if total > 1e15 {
        return Err("number out of range".into());
    }
    let hours = (total / 60.0).floor() as i64;
    let minutes = total as i64 - hours * 60;
    let sign = if h < 0.0 && total > 0.0 { "-" } else { "" };
    let hours = if format == "hh:mm" { format!("{hours:02}") } else { hours.to_string() };
    Ok(Value::String(format!("{sign}{hours}:{minutes:02}")))
}

fn round(args: &[Value]) -> R<Value> {
    let x = number("round", &args[0])?;
    let digits = match args.get(1) {
        None => 0.0,
        Some(Value::Number(n)) => {
            let d = n.as_f64().unwrap_or(f64::NAN);
            if d.fract() != 0.0 || !(0.0..=10.0).contains(&d) {
                return Err("round needs 0 to 10 digits".into());
            }
            d
        }
        Some(_) => return Err("round needs 0 to 10 digits".into()),
    };
    let mut p = 1.0;
    for _ in 0..digits as usize {
        p *= 10.0;
    }
    Ok(json::number(num(round_half_away(x * p) / p)?))
}

fn size(v: &Value) -> R<Value> {
    Ok(json::number(match v {
        Value::String(s) => s.chars().count() as f64,
        Value::Array(a) => a.len() as f64,
        Value::Object(m) => m.len() as f64,
        _ => return Err(format!("size needs a list, string or map, got {}", type_name(v))),
    }))
}

/// VAPP-206: `string(v)`: a number's or a bool's text, a string as it is.
fn string_of(v: &Value) -> R<Value> {
    Ok(Value::String(match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => num_text(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => b.to_string(),
        _ => return Err(format!("string needs a number, string or bool, got {}", type_name(v))),
    }))
}

fn list_of<'a>(f: &str, v: &'a Value) -> R<&'a Vec<Value>> {
    v.as_array().ok_or_else(|| format!("{f} needs a list, got {}", type_name(v)))
}

/// VAPP-206: a list's numbers; a null entry (a row's missing optional field)
/// is skipped.
fn numbers_of(f: &str, v: &Value) -> R<Vec<f64>> {
    let mut out = Vec::new();
    for x in list_of(f, v)? {
        match x {
            Value::Null => {}
            Value::Number(n) => out.push(n.as_f64().unwrap_or(0.0)),
            _ => return Err(format!("{f} needs a list of numbers, got {}", type_name(x))),
        }
    }
    Ok(out)
}

fn field_of<'a>(row: &'a Value, field: &str) -> &'a Value {
    row.as_object().and_then(|m| m.get(field)).unwrap_or(&Value::Null)
}

/// VAPP-206: `sortBy`'s order: numbers by value, strings by UTF-16 code unit,
/// anything else equal (the sort is stable). A missing value sorts last in
/// either order.
fn sort_by(args: &[Value]) -> R<Value> {
    use std::cmp::Ordering;
    let list = list_of("sortBy", &args[0])?;
    let field = text("sortBy", &args[1])?;
    let desc = match args.get(2) {
        None => false,
        Some(Value::String(o)) if o == "asc" => false,
        Some(Value::String(o)) if o == "desc" => true,
        Some(_) => return Err("sortBy order is asc or desc".into()),
    };
    let mut out = list.clone();
    out.sort_by(|x, y| {
        let (a, b) = (field_of(x, field), field_of(y, field));
        if a.is_null() || b.is_null() {
            return match (a.is_null(), b.is_null()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                _ => Ordering::Less,
            };
        }
        let c = match (a, b) {
            (Value::Number(p), Value::Number(q)) => p.as_f64().unwrap_or(0.0).partial_cmp(&q.as_f64().unwrap_or(0.0)).unwrap_or(Ordering::Equal),
            (Value::String(p), Value::String(q)) => utf16_cmp(p, q),
            _ => Ordering::Equal,
        };
        if desc {
            c.reverse()
        } else {
            c
        }
    });
    Ok(Value::Array(out))
}

fn group_of(args: &[Value]) -> R<Value> {
    let field = text("groupOf", &args[1])?;
    let mut out: Vec<(Value, Vec<Value>)> = Vec::new();
    for row in list_of("groupOf", &args[0])? {
        let key = field_of(row, field);
        match out.iter_mut().find(|(k, _)| deep_equal(k, key)) {
            Some((_, rows)) => rows.push(row.clone()),
            None => out.push((key.clone(), vec![row.clone()])),
        }
    }
    Ok(Value::Array(out.into_iter().map(|(key, rows)| serde_json::json!({"key": key, "rows": rows})).collect()))
}

/// The TS twin's `String.prototype.trim` set: White_Space without U+0085, plus U+FEFF.
fn trim_text(v: &str) -> &str {
    v.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{FEFF}')
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Param {
    Number,
    Text,
    Temporal,
    Sized,
    List,
    Any,
}

struct Def {
    params: &'static [Param],
    min: usize,
    variadic: bool,
    args: &'static [&'static str],
}

use Param::{Any as A, List as L, Number as N, Sized as Z, Temporal as T, Text as S};

/// THE function table (spec §4.2), positional for CEL, named (`args`) for
/// the A2UI form. Generic only: no working days, no holidays.
pub const FUNCTION_NAMES: &[&str] = &["mul", "div", "mod", "round", "floor", "ceil", "abs", "min", "max", "today", "now", "dateTime", "dateAdd", "dateDiff", "startOf", "endOf", "weekday", "isoWeek", "hours", "formatDuration", "size", "orElse", "string", "trim", "lower", "upper", "split", "join", "sumOf", "avgOf", "minOf", "maxOf", "countOf", "sortBy", "groupOf"];

/// The A2UI form's names in the bind table (`size` = the existing `len`;
/// `expr` = the string form, `{expr, vars}`).
pub const CALL_NAMES: &[&str] = &["mul", "div", "mod", "round", "floor", "ceil", "abs", "min", "max", "today", "now", "dateTime", "dateAdd", "dateDiff", "startOf", "endOf", "weekday", "isoWeek", "hours", "formatDuration", "orElse", "expr"];

/// VAPP-206: reachable from CEL only (`{call: expr}`): the bind table and the
/// catalog keep their names.
pub const CEL_ONLY: &[&str] = &["size", "string", "trim", "lower", "upper", "split", "join", "sumOf", "avgOf", "minOf", "maxOf", "countOf", "sortBy", "groupOf"];

fn def(name: &str) -> Option<Def> {
    let d = |params: &'static [Param], args: &'static [&'static str]| Def { params, min: params.len(), variadic: false, args };
    Some(match name {
        "mul" | "div" | "mod" => d(&[N, N], &["a", "b"]),
        "round" => Def { params: &[N, N], min: 1, variadic: false, args: &["value", "digits"] },
        "floor" | "ceil" | "abs" => d(&[N], &["value"]),
        "min" | "max" => Def { params: &[N], min: 1, variadic: true, args: &["values"] },
        "today" | "now" => d(&[], &[]),
        "dateTime" => Def { params: &[T, A], min: 1, variadic: false, args: &["date", "time"] },
        "dateAdd" => d(&[T, N, S], &["date", "n", "unit"]),
        "dateDiff" => d(&[T, T, S], &["a", "b", "unit"]),
        "startOf" | "endOf" => d(&[T, S], &["date", "unit"]),
        "weekday" | "isoWeek" => d(&[T], &["date"]),
        "hours" => d(&[N], &["minutes"]),
        "formatDuration" => d(&[N, S], &["hours", "format"]),
        "size" => d(&[Z], &["value"]),
        // VAPP-206: the Text and Arrays rows (CEL only, like size: `CEL_ONLY`).
        "string" => d(&[A], &["value"]),
        "trim" | "lower" | "upper" => d(&[S], &["value"]),
        "split" => d(&[S, S], &["value", "separator"]),
        "join" => Def { params: &[L, S], min: 1, variadic: false, args: &["values", "separator"] },
        "sumOf" | "avgOf" | "minOf" | "maxOf" | "countOf" => d(&[L], &["values"]),
        "sortBy" => Def { params: &[L, S, S], min: 2, variadic: false, args: &["values", "field", "order"] },
        "groupOf" => d(&[L, S], &["values", "field"]),
        // VAPP-195: the value, or the default when it is missing (null).
        "orElse" => d(&[A, A], &["value", "default"]),
        _ => return None,
    })
}

fn arity_error(name: &str, def: &Def, n: usize) -> Option<String> {
    if def.variadic {
        return (n < 1).then(|| format!("{name} takes at least 1 argument"));
    }
    let (min, max) = (def.min, def.params.len());
    if n >= min && n <= max {
        return None;
    }
    Some(if min == max { format!("{name} takes {min} argument{}", if min == 1 { "" } else { "s" }) } else { format!("{name} takes {min} or {max} arguments") })
}

fn run(name: &str, args: &[Value], env: &Env) -> R<Value> {
    let n = |f: fn(f64) -> f64| -> R<Value> { Ok(json::number(num(f(number(name, &args[0])?))?)) };
    match name {
        "mul" => Ok(json::number(num(number("mul", &args[0])? * number("mul", &args[1])?)?)),
        "div" => {
            let (x, y) = (number("div", &args[0])?, number("div", &args[1])?);
            if y == 0.0 {
                return Err("division by zero".into());
            }
            Ok(json::number(num(x / y)?))
        }
        "mod" => {
            let (x, y) = (number("mod", &args[0])?, number("mod", &args[1])?);
            if y == 0.0 {
                return Err("modulo by zero".into());
            }
            Ok(json::number(num(x % y)?))
        }
        "round" => round(args),
        "floor" => n(f64::floor),
        "ceil" => n(f64::ceil),
        "abs" => n(f64::abs),
        "min" | "max" => {
            let mut out = if name == "min" { f64::INFINITY } else { f64::NEG_INFINITY };
            for v in args {
                let x = number(name, v)?;
                out = if name == "min" { out.min(x) } else { out.max(x) };
            }
            Ok(json::number(num(out)?))
        }
        "today" => Ok(Value::String(day_text(floor_day(to_local(env, env.now)))?)),
        "now" => Ok(Value::String(format_timestamp(env.now)?)),
        "dateTime" => date_time(args, env),
        "dateAdd" => date_add(args, env),
        "dateDiff" => date_diff(args, env),
        "startOf" | "endOf" => start_end(name, args, env),
        "weekday" => {
            let d = local_day("weekday", &args[0], env)?;
            Ok(json::number(match weekday_of(d.y, d.m, d.d) {
                0 => 7.0,
                w => w as f64,
            }))
        }
        "isoWeek" => {
            let d = local_day("isoWeek", &args[0], env)?;
            Ok(json::number(iso_week(d.y, d.m, d.d).1 as f64))
        }
        "hours" => Ok(json::number(num(number("hours", &args[0])? / 60.0)?)),
        "formatDuration" => format_duration(args),
        "size" => size(&args[0]),
        "string" => string_of(&args[0]),
        "trim" => Ok(Value::String(trim_text(text("trim", &args[0])?).to_string())),
        "lower" => Ok(Value::String(text("lower", &args[0])?.to_lowercase())),
        "upper" => Ok(Value::String(text("upper", &args[0])?.to_uppercase())),
        "split" => {
            let value = text("split", &args[0])?;
            let separator = text("split", &args[1])?;
            if separator.is_empty() {
                return Err("split needs a separator".into());
            }
            Ok(Value::Array(value.split(separator).map(|s| Value::String(s.to_string())).collect()))
        }
        "join" => {
            let separator = match args.get(1) {
                Some(v) => text("join", v)?,
                None => "",
            };
            let mut parts: Vec<&str> = Vec::new();
            for x in list_of("join", &args[0])? {
                match x {
                    Value::String(s) => parts.push(s),
                    _ => return Err(format!("join needs a list of strings, got {}", type_name(x))),
                }
            }
            Ok(Value::String(parts.join(separator)))
        }
        "sumOf" => Ok(json::number(num(numbers_of("sumOf", &args[0])?.iter().sum::<f64>())?)),
        "avgOf" => {
            let ns = numbers_of("avgOf", &args[0])?;
            if ns.is_empty() {
                return Ok(Value::Null);
            }
            Ok(json::number(num(ns.iter().sum::<f64>() / ns.len() as f64)?))
        }
        "minOf" | "maxOf" => {
            let ns = numbers_of(name, &args[0])?;
            let picked = ns.into_iter().reduce(|a, b| if (name == "minOf" && b < a) || (name == "maxOf" && b > a) { b } else { a });
            Ok(picked.map_or(Value::Null, json::number))
        }
        "countOf" => Ok(json::number(list_of("countOf", &args[0])?.iter().filter(|x| !x.is_null()).count() as f64)),
        "sortBy" => sort_by(args),
        "groupOf" => group_of(args),
        "orElse" => Ok(if args[0].is_null() { args.get(1).cloned().unwrap_or(Value::Null) } else { args[0].clone() }),
        _ => Err(format!("unknown function {name}")),
    }
}

/// The A2UI function form `{call: name, args}` with its arguments resolved.
/// Outer `None` = not an expression function; inner `None` = no value (an
/// error, like every bind-time function). VAPP-195: a failed call shows its
/// `default` (absent = no value).
pub fn call_expression_function(name: &str, args: &Map<String, Value>, env: &ExpressionEnv) -> Option<Option<Value>> {
    let fallback = || Some(args.get("default").cloned());
    if name == "expr" {
        let Some(Value::String(expr)) = args.get("expr") else { return fallback() };
        let empty = Map::new();
        let vars = args.get("vars").and_then(Value::as_object).unwrap_or(&empty);
        return evaluate_expression(expr, vars, env).map_or_else(|_| fallback(), |v| Some(Some(v)));
    }
    if CEL_ONLY.contains(&name) {
        return None;
    }
    let def = def(name)?;
    let positional: Vec<Value> = if def.variadic {
        args.get("values").and_then(Value::as_array).cloned().unwrap_or_default()
    } else {
        let mut list: Vec<Option<&Value>> = def.args.iter().map(|k| args.get(*k)).collect();
        while list.len() > def.min && list.last().is_some_and(Option::is_none) {
            list.pop();
        }
        list.into_iter().map(|v| v.cloned().unwrap_or(Value::Null)).collect()
    };
    if arity_error(name, &def, positional.len()).is_some() {
        return fallback();
    }
    run(name, &positional, &Env::of(env)).map_or_else(|_| fallback(), |v| Some(Some(v)))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// A parsed expression (`at` = the code point it starts at).
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Lit(Value, usize),
    Id(String, usize),
    List(Vec<Node>, usize),
    /// VAPP-206: `{'k': e}`, string keys in source order.
    Map(Vec<(String, Node)>, usize),
    Field(Box<Node>, String, usize),
    Index(Box<Node>, Box<Node>, usize),
    Unary(char, Box<Node>, usize),
    Bin(&'static str, Box<Node>, Box<Node>, usize),
    Cond(Box<Node>, Box<Node>, Box<Node>, usize),
    Call(String, Vec<Node>, usize),
    Macro(String, Box<Node>, String, Box<Node>, usize),
    /// VAPP-195: `has(a.b)`, CEL's presence test (the node is a `Field`).
    Has(Box<Node>, usize),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Id(String),
    Op(&'static str),
    End,
}

#[derive(Debug, Clone)]
struct Token {
    t: Tok,
    at: usize,
    text: String,
}

const OPS2: &[&str] = &["==", "!=", "<=", ">=", "&&", "||"];
const OPS1: &[&str] = &["+", "-", "*", "/", "%", "<", ">", "!", "(", ")", "[", "]", "{", "}", ",", ".", "?", ":"];

fn is_digit(c: Option<&char>) -> bool {
    c.is_some_and(char::is_ascii_digit)
}
fn is_id_start(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
}
fn is_id_part(c: Option<&char>) -> bool {
    is_id_start(c) || is_digit(c)
}

fn tokenize(cs: &[char]) -> R<Vec<Token>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            i += 1;
            continue;
        }
        let at = i;
        if c.is_ascii_digit() {
            while is_digit(cs.get(i)) {
                i += 1;
            }
            if cs.get(i) == Some(&'.') && is_digit(cs.get(i + 1)) {
                i += 1;
                while is_digit(cs.get(i)) {
                    i += 1;
                }
            }
            if matches!(cs.get(i), Some('e' | 'E')) && (is_digit(cs.get(i + 1)) || (matches!(cs.get(i + 1), Some('+' | '-')) && is_digit(cs.get(i + 2)))) {
                i += 2;
                while is_digit(cs.get(i)) {
                    i += 1;
                }
            }
            let text: String = cs[at..i].iter().collect();
            let v = text.parse::<f64>().unwrap_or(f64::INFINITY);
            out.push(Token { t: Tok::Num(v), at, text });
            continue;
        }
        if c == '\'' || c == '"' {
            i += 1;
            let mut s = String::new();
            loop {
                let d = cs.get(i).copied();
                match d {
                    None | Some('\n') => return Err(format!("at {at}: unterminated string")),
                    Some(d) if d == c => break,
                    Some('\\') => {
                        let e = cs.get(i + 1).copied();
                        let mapped = match e {
                            Some('\\') => '\\',
                            Some('\'') => '\'',
                            Some('"') => '"',
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('r') => '\r',
                            _ => return Err(format!("at {i}: unknown escape \\{}", e.map(String::from).unwrap_or_default())),
                        };
                        s.push(mapped);
                        i += 2;
                    }
                    Some(d) => {
                        s.push(d);
                        i += 1;
                    }
                }
            }
            i += 1;
            out.push(Token { t: Tok::Str(s), at, text: cs[at..i].iter().collect() });
            continue;
        }
        if is_id_start(Some(&c)) {
            while is_id_part(cs.get(i)) {
                i += 1;
            }
            let text: String = cs[at..i].iter().collect();
            out.push(Token { t: Tok::Id(text.clone()), at, text });
            continue;
        }
        let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
        if let Some(op) = OPS2.iter().find(|o| **o == two) {
            out.push(Token { t: Tok::Op(op), at, text: two });
            i += 2;
            continue;
        }
        if let Some(op) = OPS1.iter().find(|o| o.starts_with(c)) {
            out.push(Token { t: Tok::Op(op), at, text: c.to_string() });
            i += 1;
            continue;
        }
        return Err(format!("at {at}: unexpected {c}"));
    }
    out.push(Token { t: Tok::End, at: cs.len(), text: "end".into() });
    Ok(out)
}

const MACROS: &[&str] = &["map", "filter", "all", "exists"];

struct Parser {
    tokens: Vec<Token>,
    i: usize,
    depth: usize,
    in_body: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.i]
    }
    fn is_op(&self, v: &str) -> bool {
        matches!(&self.peek().t, Tok::Op(o) if *o == v)
    }
    fn next(&mut self) -> Token {
        let t = self.tokens[self.i].clone();
        self.i += 1;
        t
    }
    fn unexpected<T>(&self) -> R<T> {
        let t = self.peek();
        Err(format!("at {}: unexpected {}", t.at, t.text))
    }
    fn expect(&mut self, v: &str) -> R<Token> {
        if !self.is_op(v) {
            return self.unexpected();
        }
        Ok(self.next())
    }
    fn enter(&mut self, at: usize) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!("at {at}: expression is nested too deeply"));
        }
        Ok(())
    }

    fn parse_all(&mut self) -> R<Node> {
        let node = self.expr()?;
        if self.peek().t != Tok::End {
            return self.unexpected();
        }
        Ok(node)
    }

    fn expr(&mut self) -> R<Node> {
        crate::deep(|| self.expr_inner())
    }

    fn expr_inner(&mut self) -> R<Node> {
        let at = self.peek().at;
        self.enter(at)?;
        let test = self.or()?;
        let mut out = test;
        if self.is_op("?") {
            self.next();
            let then = self.expr()?;
            self.expect(":")?;
            let otherwise = self.expr()?;
            out = Node::Cond(Box::new(out), Box::new(then), Box::new(otherwise), at);
        }
        self.depth -= 1;
        Ok(out)
    }

    fn binary(&mut self, ops: &[&'static str], operand: fn(&mut Parser) -> R<Node>) -> R<Node> {
        let mut a = operand(self)?;
        loop {
            let t = self.peek().clone();
            let Tok::Op(op) = t.t else { return Ok(a) };
            let Some(op) = ops.iter().find(|o| **o == op) else { return Ok(a) };
            self.next();
            let b = operand(self)?;
            a = Node::Bin(op, Box::new(a), Box::new(b), t.at);
        }
    }
    fn or(&mut self) -> R<Node> {
        self.binary(&["||"], Parser::and)
    }
    fn and(&mut self) -> R<Node> {
        self.binary(&["&&"], Parser::rel)
    }
    fn rel(&mut self) -> R<Node> {
        self.binary(&["==", "!=", "<", "<=", ">", ">="], Parser::add)
    }
    fn add(&mut self) -> R<Node> {
        self.binary(&["+", "-"], Parser::mul)
    }
    fn mul(&mut self) -> R<Node> {
        self.binary(&["*", "/", "%"], Parser::unary)
    }

    fn unary(&mut self) -> R<Node> {
        let t = self.peek().clone();
        if let Tok::Op(op @ ("!" | "-")) = t.t {
            self.next();
            self.enter(t.at)?;
            let arg = crate::deep(|| self.unary())?;
            self.depth -= 1;
            return Ok(Node::Unary(op.chars().next().unwrap_or('!'), Box::new(arg), t.at));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> R<Node> {
        let mut node = self.primary()?;
        loop {
            if self.is_op(".") {
                self.next();
                let name = self.peek().clone();
                let Tok::Id(field) = name.t.clone() else { return self.unexpected() };
                self.next();
                if self.is_op("(") {
                    if !MACROS.contains(&field.as_str()) {
                        if let Some(dotted) = dotted_name(&node) {
                            let full = format!("{dotted}.{field}");
                            let effect = function_effect(&full);
                            if effect != FunctionEffect::Pure {
                                return Err(format!("at {}: {}: an expression is pure, use an action step", root_at(&node), effect_words(&full, effect)));
                            }
                        }
                        return Err(format!("at {}: unknown method {field}", name.at));
                    }
                    if self.in_body > 0 {
                        return Err(format!("at {}: nested comprehensions are not allowed", name.at));
                    }
                    self.next();
                    let v = self.peek().clone();
                    let comma = matches!(self.tokens.get(self.i + 1).map(|t| &t.t), Some(Tok::Op(",")));
                    let Tok::Id(var) = v.t.clone() else { return Err(format!("at {}: {field} needs a name first", v.at)) };
                    if !comma {
                        return Err(format!("at {}: {field} needs a name first", v.at));
                    }
                    self.next();
                    self.next();
                    self.in_body += 1;
                    let body = self.expr()?;
                    self.in_body -= 1;
                    self.expect(")")?;
                    node = Node::Macro(field, Box::new(node), var, Box::new(body), name.at);
                } else {
                    node = Node::Field(Box::new(node), field, name.at);
                }
                continue;
            }
            if self.is_op("[") {
                let at = self.next().at;
                let index = self.expr()?;
                self.expect("]")?;
                node = Node::Index(Box::new(node), Box::new(index), at);
                continue;
            }
            return Ok(node);
        }
    }

    fn list(&mut self, close: &str) -> R<Vec<Node>> {
        let mut items = Vec::new();
        if self.is_op(close) {
            self.next();
            return Ok(items);
        }
        loop {
            items.push(self.expr()?);
            if self.is_op(",") {
                self.next();
                if self.is_op(close) && close == "]" {
                    self.next();
                    return Ok(items);
                }
                continue;
            }
            self.expect(close)?;
            return Ok(items);
        }
    }

    /// VAPP-206: `{'k': e, …}`: string keys, no duplicates, `?` never (it
    /// spells an optional type).
    fn map(&mut self) -> R<Vec<(String, Node)>> {
        let mut entries: Vec<(String, Node)> = Vec::new();
        while !self.is_op("}") {
            let key = self.peek().clone();
            let Tok::Str(name) = key.t else {
                return Err(format!("at {}: a map key is a string", key.at));
            };
            if name == "?" {
                return Err(format!("at {}: ? is not a map key", key.at));
            }
            if entries.iter().any(|(k, _)| *k == name) {
                return Err(format!("at {}: duplicate key {name}", key.at));
            }
            self.next();
            self.expect(":")?;
            let value = self.expr()?;
            entries.push((name, value));
            if !self.is_op(",") {
                break;
            }
            self.next();
        }
        self.expect("}")?;
        Ok(entries)
    }

    fn primary(&mut self) -> R<Node> {
        let t = self.peek().clone();
        match t.t {
            Tok::Num(v) => {
                self.next();
                if !v.is_finite() {
                    return Err(format!("at {}: number out of range", t.at));
                }
                Ok(Node::Lit(json::number(v), t.at))
            }
            Tok::Str(s) => {
                self.next();
                Ok(Node::Lit(Value::String(s), t.at))
            }
            Tok::Id(name) => {
                self.next();
                match name.as_str() {
                    "true" | "false" => return Ok(Node::Lit(Value::Bool(name == "true"), t.at)),
                    "null" => return Ok(Node::Lit(Value::Null, t.at)),
                    _ => {}
                }
                // VAPP-195: CEL's presence test, `has(a.b)`.
                if name == "has" && self.is_op("(") {
                    self.next();
                    let arg = self.expr()?;
                    self.expect(")")?;
                    if !matches!(arg, Node::Field(..)) {
                        let hint = match &arg {
                            Node::Id(name, _) => format!("; for a name write {name} != null"),
                            _ => String::new(),
                        };
                        return Err(format!("at {}: has needs a field: has(a.b){hint}", t.at));
                    }
                    return Ok(Node::Has(Box::new(arg), t.at));
                }
                if self.is_op("(") {
                    if def(&name).is_none() {
                        let effect = function_effect(&name);
                        if effect != FunctionEffect::Pure {
                            return Err(format!("at {}: {}: an expression is pure, use an action step", t.at, effect_words(&name, effect)));
                        }
                        return Err(format!("at {}: unknown function {name}", t.at));
                    }
                    self.next();
                    let args = self.list(")")?;
                    return Ok(Node::Call(name, args, t.at));
                }
                Ok(Node::Id(name, t.at))
            }
            Tok::Op("(") => {
                self.next();
                let inner = self.expr()?;
                self.expect(")")?;
                Ok(inner)
            }
            Tok::Op("[") => {
                self.next();
                Ok(Node::List(self.list("]")?, t.at))
            }
            Tok::Op("{") => {
                self.next();
                Ok(Node::Map(self.map()?, t.at))
            }
            _ => self.unexpected(),
        }
    }
}

/// `a.b.c` as a name (a chain of plain names), else `None`.
fn dotted_name(node: &Node) -> Option<String> {
    match node {
        Node::Id(name, _) => Some(name.clone()),
        Node::Field(of, name, _) => dotted_name(of).map(|of| format!("{of}.{name}")),
        _ => None,
    }
}

/// VAPP-214: the free names an expression reads, sorted (a macro's own
/// variable is not free inside its body).
pub fn expression_names(node: &Node) -> Vec<String> {
    fn walk(n: &Node, bound: &mut Vec<String>, out: &mut std::collections::BTreeSet<String>) {
        match n {
            Node::Lit(..) => {}
            Node::Id(name, _) => {
                if !bound.iter().any(|b| b == name) {
                    out.insert(name.clone());
                }
            }
            Node::List(items, _) | Node::Call(_, items, _) => items.iter().for_each(|x| walk(x, bound, out)),
            Node::Map(entries, _) => entries.iter().for_each(|(_, x)| walk(x, bound, out)),
            Node::Field(of, _, _) | Node::Has(of, _) | Node::Unary(_, of, _) => walk(of, bound, out),
            Node::Index(of, index, _) => {
                walk(of, bound, out);
                walk(index, bound, out);
            }
            Node::Bin(_, a, b, _) => {
                walk(a, bound, out);
                walk(b, bound, out);
            }
            Node::Cond(test, then, other, _) => {
                walk(test, bound, out);
                walk(then, bound, out);
                walk(other, bound, out);
            }
            Node::Macro(_, of, v, body, _) => {
                walk(of, bound, out);
                bound.push(v.clone());
                walk(body, bound, out);
                bound.pop();
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(node, &mut Vec::new(), &mut out);
    out.into_iter().collect()
}

/// VAPP-195: a value's path as written (`a.b[0].c`, `m['k']`): names,
/// fields and literal indexes; `None` for anything else.
pub fn path_text(node: &Node) -> Option<String> {
    match node {
        Node::Id(name, _) => Some(name.clone()),
        Node::Field(of, name, _) => path_text(of).map(|of| format!("{of}.{name}")),
        Node::Index(of, index, _) => {
            let of = path_text(of)?;
            match literal(index) {
                Some(Value::Number(n)) => Some(format!("{of}[{}]", num_text(n.as_f64().unwrap_or(0.0)))),
                Some(Value::String(k)) => Some(format!("{of}['{k}']")),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The code point a node starts at (its own `at`).
fn node_at(node: &Node) -> usize {
    match node {
        Node::Lit(_, at) | Node::Id(_, at) | Node::List(_, at) | Node::Map(_, at) | Node::Field(_, _, at) | Node::Index(_, _, at) | Node::Unary(_, _, at) | Node::Bin(_, _, _, at) | Node::Cond(_, _, _, at) | Node::Call(_, _, at) | Node::Macro(_, _, _, _, at) | Node::Has(_, at) => *at,
    }
}

fn root_at(node: &Node) -> usize {
    match node {
        Node::Field(of, _, _) => root_at(of),
        _ => node_at(node),
    }
}

/// Parse once (at package validation, or before evaluating): the tree, or
/// the first syntax / cost error (`at <code point>: <reason>`).
pub fn parse_expression(source: &str) -> Result<Node, String> {
    let cs: Vec<char> = source.chars().collect();
    if cs.len() > MAX_LENGTH {
        return Err(format!("expression is longer than {MAX_LENGTH} characters"));
    }
    Parser { tokens: tokenize(&cs)?, i: 0, depth: 0, in_body: 0 }.parse_all()
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

struct Scope<'a> {
    name: &'a str,
    value: &'a Value,
    parent: Option<&'a Scope<'a>>,
}

struct Evaluator<'v> {
    vars: &'v Map<String, Value>,
    env: Env<'v>,
    steps: usize,
    max_steps: usize,
}

impl Evaluator<'_> {
    fn step(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err("step limit exceeded".into());
        }
        Ok(())
    }

    fn ev(&mut self, node: &Node, scope: Option<&Scope>) -> R<Value> {
        crate::deep(|| self.ev_inner(node, scope))
    }

    fn ev_inner(&mut self, node: &Node, scope: Option<&Scope>) -> R<Value> {
        self.step()?;
        match node {
            Node::Lit(v, _) => Ok(v.clone()),
            Node::List(items, _) => items.iter().map(|i| self.ev(i, scope)).collect::<R<Vec<_>>>().map(Value::Array),
            Node::Map(entries, _) => {
                let mut out = Map::new();
                for (k, v) in entries {
                    out.insert(k.clone(), self.ev(v, scope)?);
                }
                Ok(Value::Object(out))
            }
            Node::Id(name, _) => {
                let mut s = scope;
                while let Some(sc) = s {
                    if sc.name == name {
                        return Ok(sc.value.clone());
                    }
                    s = sc.parent;
                }
                self.vars.get(name).cloned().ok_or_else(|| format!("unknown name {name}"))
            }
            Node::Field(of, name, _) => match self.ev(of, scope)? {
                Value::Null => Ok(Value::Null),
                Value::Object(m) => Ok(m.get(name).cloned().unwrap_or(Value::Null)),
                other => Err(format!("no field {name} on {}", type_name(&other))),
            },
            Node::Has(field, _) => {
                let Node::Field(receiver, name, _) = field.as_ref() else { return Err("has needs a field: has(a.b)".into()) };
                match self.ev(receiver, scope)? {
                    Value::Null => Ok(Value::Bool(false)),
                    Value::Object(m) => Ok(Value::Bool(m.get(name).is_some_and(|v| !v.is_null()))),
                    other => Err(format!("no field {name} on {}", type_name(&other))),
                }
            }
            Node::Index(of, index, _) => {
                let of = self.ev(of, scope)?;
                let index = self.ev(index, scope)?;
                match of {
                    Value::Array(list) => {
                        let Value::Number(n) = &index else { return Err(format!("a list index is a number, got {}", type_name(&index))) };
                        let n = n.as_f64().unwrap_or(0.0);
                        if n.fract() != 0.0 {
                            return Err(format!("a list index is a whole number, got {}", num_text(n)));
                        }
                        // VAPP-204: an index past either end is null, like a missing field.
                        if n < 0.0 || n >= list.len() as f64 {
                            return Ok(Value::Null);
                        }
                        Ok(list[n as usize].clone())
                    }
                    Value::Object(m) => {
                        let Value::String(k) = &index else { return Err(format!("a map key is a string, got {}", type_name(&index))) };
                        Ok(m.get(k).cloned().unwrap_or(Value::Null))
                    }
                    Value::Null => Ok(Value::Null),
                    other => Err(format!("cannot index {}", type_name(&other))),
                }
            }
            Node::Unary(op, arg, _) => {
                let v = self.ev(arg, scope)?;
                if *op == '-' {
                    let Value::Number(n) = &v else { return Err(format!("cannot apply - to {}", type_name(&v))) };
                    return Ok(json::number(num(-n.as_f64().unwrap_or(0.0))?));
                }
                let Value::Bool(b) = v else { return Err(format!("cannot apply ! to {}", type_name(&v))) };
                Ok(Value::Bool(!b))
            }
            Node::Cond(test, then, otherwise, _) => {
                let t = self.ev(test, scope)?;
                let Value::Bool(t) = t else { return Err(format!("cannot use {} as a condition", type_name(&t))) };
                self.ev(if t { then } else { otherwise }, scope)
            }
            Node::Bin(op, a, b, _) => self.binary(op, a, b, scope),
            Node::Call(name, args, _) => {
                let args = args.iter().map(|a| self.ev(a, scope)).collect::<R<Vec<_>>>()?;
                let d = def(name).ok_or_else(|| format!("unknown function {name}"))?;
                if let Some(e) = arity_error(name, &d, args.len()) {
                    return Err(e);
                }
                run(name, &args, &self.env)
            }
            Node::Macro(f, of, var, body, _) => {
                let of = self.ev(of, scope)?;
                let Value::Array(list) = of else { return Err(format!("{f} needs a list, got {}", type_name(&of))) };
                let mut out = Vec::new();
                for item in &list {
                    self.step()?;
                    let inner = Scope { name: var, value: item, parent: scope };
                    let v = self.ev(body, Some(&inner))?;
                    if f == "map" {
                        out.push(v);
                        continue;
                    }
                    let Value::Bool(b) = v else { return Err(format!("{f} needs a bool, got {}", type_name(&v))) };
                    match f.as_str() {
                        "filter" if b => out.push(item.clone()),
                        "all" if !b => return Ok(Value::Bool(false)),
                        "exists" if b => return Ok(Value::Bool(true)),
                        _ => {}
                    }
                }
                Ok(match f.as_str() {
                    "all" => Value::Bool(true),
                    "exists" => Value::Bool(false),
                    _ => Value::Array(out),
                })
            }
        }
    }

    fn binary(&mut self, op: &str, a: &Node, b: &Node, scope: Option<&Scope>) -> R<Value> {
        if op == "&&" || op == "||" {
            let x = self.ev(a, scope)?;
            let Value::Bool(x) = x else { return Err(format!("cannot apply {op} to {}", type_name(&x))) };
            if (op == "&&" && !x) || (op == "||" && x) {
                return Ok(Value::Bool(x));
            }
            let y = self.ev(b, scope)?;
            let Value::Bool(_) = y else { return Err(format!("cannot apply {op} to {}", type_name(&y))) };
            return Ok(y);
        }
        let x = self.ev(a, scope)?;
        let y = self.ev(b, scope)?;
        match op {
            "==" => Ok(Value::Bool(deep_equal(&x, &y))),
            "!=" => Ok(Value::Bool(!deep_equal(&x, &y))),
            "<" | "<=" | ">" | ">=" => {
                let c = match (&x, &y) {
                    (Value::Number(p), Value::Number(q)) => p.as_f64().partial_cmp(&q.as_f64()).unwrap_or(std::cmp::Ordering::Equal),
                    (Value::String(p), Value::String(q)) => utf16_cmp(p, q),
                    _ => return Err(format!("cannot compare {} and {}", type_name(&x), type_name(&y))),
                };
                use std::cmp::Ordering::*;
                Ok(Value::Bool(match op {
                    "<" => c == Less,
                    "<=" => c != Greater,
                    ">" => c == Greater,
                    _ => c != Less,
                }))
            }
            "+" => match (x, y) {
                (Value::Number(p), Value::Number(q)) => Ok(json::number(num(p.as_f64().unwrap_or(0.0) + q.as_f64().unwrap_or(0.0))?)),
                (Value::String(p), Value::String(q)) => Ok(Value::String(p + &q)),
                (Value::Array(mut p), Value::Array(q)) => {
                    p.extend(q);
                    Ok(Value::Array(p))
                }
                (x, y) => Err(format!("cannot apply + to {} and {}", type_name(&x), type_name(&y))),
            },
            _ => {
                let (Value::Number(p), Value::Number(q)) = (&x, &y) else { return Err(format!("cannot apply {op} to {} and {}", type_name(&x), type_name(&y))) };
                let (p, q) = (p.as_f64().unwrap_or(0.0), q.as_f64().unwrap_or(0.0));
                match op {
                    "-" => Ok(json::number(num(p - q)?)),
                    "*" => Ok(json::number(num(p * q)?)),
                    _ => {
                        if q == 0.0 {
                            return Err(if op == "/" { "division by zero" } else { "modulo by zero" }.into());
                        }
                        Ok(json::number(num(if op == "/" { p / q } else { p % q })?))
                    }
                }
            }
        }
    }
}

/// Evaluate an expression over `vars`: the value, or the error (a syntax,
/// type, range or cost error).
pub fn evaluate_expression(source: &str, vars: &Map<String, Value>, env: &ExpressionEnv) -> Result<Value, String> {
    let ast = parse_expression(source)?;
    evaluate_node(&ast, vars, env)
}

/// Evaluate a parsed tree (see [`parse_expression`]).
pub fn evaluate_node<'a>(ast: &Node, vars: &'a Map<String, Value>, env: &ExpressionEnv<'a>) -> Result<Value, String> {
    let mut e = Evaluator { vars, env: Env::of(env), steps: 0, max_steps: env.max_steps.unwrap_or(MAX_STEPS) };
    e.ev(ast, None)
}

/// VAPP-194: evaluate a parsed tree and count its steps (an action spends
/// one budget over all of its expressions): the result and the steps used.
pub fn evaluate_counted<'a>(ast: &Node, vars: &'a Map<String, Value>, env: &ExpressionEnv<'a>) -> (Result<Value, String>, usize) {
    let mut e = Evaluator { vars, env: Env::of(env), steps: 0, max_steps: env.max_steps.unwrap_or(MAX_STEPS) };
    let result = e.ev(ast, None);
    (result, e.steps)
}

// ---------------------------------------------------------------------------
// Type checking (package validation)
// ---------------------------------------------------------------------------

/// VAPP-195 (E2): a value that may be missing is `T?`, spelled `{"?": T}`
/// (a row never has a field named `?`).
pub fn is_optional(t: &Value) -> bool {
    matches!(t, Value::Object(m) if m.len() == 1 && m.contains_key("?"))
}

/// `T?` (null and an optional stay as they are).
pub fn optional_type(t: Value) -> Value {
    if is_optional(&t) || is(&t, "null") {
        return t;
    }
    let mut m = Map::new();
    m.insert("?".into(), t);
    Value::Object(m)
}

/// `T` of a `T?`.
pub fn required_type(t: &Value) -> Value {
    match t {
        Value::Object(m) if is_optional(t) => m["?"].clone(),
        _ => t.clone(),
    }
}

/// The name a type prints as in an error: a list `list`, a row `map`, an
/// optional `T?`.
pub fn type_display(t: &Value) -> String {
    match t {
        Value::Object(m) if is_optional(t) => format!("{}?", type_display(&m["?"])),
        Value::String(s) => s.clone(),
        Value::Array(_) => "list".into(),
        _ => "map".into(),
    }
}

fn is_row(t: &Value) -> bool {
    t.is_object() && !is_optional(t)
}
fn is_textual(t: &Value) -> bool {
    matches!(t.as_str(), Some("string" | "date" | "timestamp"))
}
fn loose(t: &Value) -> bool {
    matches!(t.as_str(), Some("dyn" | "null"))
}
fn is(t: &Value, name: &str) -> bool {
    t.as_str() == Some(name)
}
fn ty(name: &str) -> Value {
    Value::String(name.into())
}

fn accepts(param: Param, t: &Value) -> bool {
    if param == Param::Any || loose(t) {
        return true;
    }
    match param {
        Param::Number => is(t, "number"),
        Param::Text | Param::Temporal => is_textual(t),
        Param::Sized => is(t, "string") || is(t, "map") || t.is_array() || is_row(t),
        Param::List => t.is_array(),
        Param::Any => true,
    }
}

fn param_name(p: Param) -> &'static str {
    match p {
        Param::Number => "a number",
        Param::Text => "a string",
        Param::Temporal => "a date",
        Param::Sized => "a list, string or map",
        Param::List => "a list",
        Param::Any => "a value",
    }
}

struct TypeScope<'a> {
    name: &'a str,
    ty: Value,
    parent: Option<&'a TypeScope<'a>>,
}

fn literal(node: &Node) -> Option<&Value> {
    match node {
        Node::Lit(v, _) => Some(v),
        _ => None,
    }
}

/// The paths a test proves present when it is true (`yes`) / when it is
/// false (`no`): `has(p)`, `p != null`, `p == null`, through `&&`, `||`
/// and `!`.
fn guards(node: &Node) -> (Vec<String>, Vec<String>) {
    match node {
        Node::Has(field, _) => (path_text(field).into_iter().collect(), Vec::new()),
        Node::Bin(op @ ("==" | "!="), a, b, _) => {
            let side = if matches!(a.as_ref(), Node::Lit(Value::Null, _)) {
                Some(b)
            } else if matches!(b.as_ref(), Node::Lit(Value::Null, _)) {
                Some(a)
            } else {
                None
            };
            match side.and_then(|s| path_text(s)) {
                None => (Vec::new(), Vec::new()),
                Some(p) if *op == "!=" => (vec![p], Vec::new()),
                Some(p) => (Vec::new(), vec![p]),
            }
        }
        Node::Bin("&&", a, b, _) => {
            let (mut yes, _) = guards(a);
            yes.extend(guards(b).0);
            (yes, Vec::new())
        }
        Node::Bin("||", a, b, _) => {
            let (_, mut no) = guards(a);
            no.extend(guards(b).1);
            (Vec::new(), no)
        }
        Node::Unary('!', arg, _) => {
            let (yes, no) = guards(arg);
            (no, yes)
        }
        _ => (Vec::new(), Vec::new()),
    }
}

/// An unguarded use of a value that may be missing.
fn missing<T>(node: &Node) -> R<T> {
    let p = path_text(node).unwrap_or_else(|| format!("the value at {}", node_at(node)));
    Err(format!("{p} may be missing: guard it with has(), a null check or orElse()"))
}

struct Checker<'t> {
    types: &'t Map<String, Value>,
    /// The paths a guard proved present here (`has(r.f) ? r.f + 1 : 0`).
    present: std::cell::RefCell<Vec<String>>,
}

impl Checker<'_> {
    /// Type `node` with `paths` proved present.
    fn within(&self, paths: Vec<String>, node: &Node, scope: Option<&TypeScope>) -> R<Value> {
        let before = self.present.borrow().clone();
        self.present.borrow_mut().extend(paths);
        let out = self.tc(node, scope);
        *self.present.borrow_mut() = before;
        out
    }

    /// The type of a use that must not be missing.
    fn need(&self, node: &Node, scope: Option<&TypeScope>) -> R<Value> {
        let t = self.tc(node, scope)?;
        if is_optional(&t) {
            return missing(node);
        }
        Ok(t)
    }

    fn tc(&self, node: &Node, scope: Option<&TypeScope>) -> R<Value> {
        let t = crate::deep(|| self.tc_inner(node, scope))?;
        let p = if matches!(node, Node::Id(..) | Node::Field(..) | Node::Index(..)) { path_text(node) } else { None };
        Ok(match p {
            Some(p) if self.present.borrow().contains(&p) => required_type(&t),
            _ => t,
        })
    }

    fn tc_inner(&self, node: &Node, scope: Option<&TypeScope>) -> R<Value> {
        match node {
            Node::Lit(v, _) => Ok(ty(type_name(v))),
            Node::List(items, _) => {
                let items = items.iter().map(|i| self.tc(i, scope)).collect::<R<Vec<_>>>()?;
                Ok(Value::Array(vec![match items.first() {
                    None => ty("dyn"),
                    Some(first) if items.iter().all(|t| t == first) => first.clone(),
                    Some(_) => ty("dyn"),
                }]))
            }
            Node::Map(entries, _) => {
                let mut out = Map::new();
                for (k, v) in entries {
                    out.insert(k.clone(), self.tc(v, scope)?);
                }
                Ok(Value::Object(out))
            }
            Node::Id(name, _) => {
                let mut s = scope;
                while let Some(sc) = s {
                    if sc.name == name {
                        return Ok(sc.ty.clone());
                    }
                    s = sc.parent;
                }
                self.types.get(name).cloned().ok_or_else(|| format!("unknown name {name}"))
            }
            Node::Field(..) | Node::Has(..) => {
                let has = matches!(node, Node::Has(..));
                let field = match node {
                    Node::Has(field, _) => field.as_ref(),
                    _ => node,
                };
                let Node::Field(of, name, _) = field else { return Err("has needs a field: has(a.b)".into()) };
                let raw = self.tc(of, scope)?;
                let of = required_type(&raw);
                // VAPP-204: a field of a map may be missing (`dyn?`).
                let t = if is(&of, "map") {
                    optional_type(ty("dyn"))
                } else if matches!(of.as_str(), Some("dyn" | "null")) {
                    ty("dyn")
                } else if let Value::Object(fields) = &of {
                    fields.get(name).cloned().ok_or_else(|| format!("unknown field {name}"))?
                } else {
                    return Err(format!("no field {name} on {}", type_display(&of)));
                };
                if has {
                    return Ok(ty("bool"));
                }
                Ok(if is_optional(&raw) { optional_type(t) } else { t })
            }
            Node::Index(of, index, _) => {
                let raw = self.tc(of, scope)?;
                let of_t = required_type(&raw);
                let index_t = self.need(index, scope)?;
                let wrap = |t: Value| if is_optional(&raw) { optional_type(t) } else { t };
                if let Value::Array(item) = &of_t {
                    if !loose(&index_t) && !is(&index_t, "number") {
                        return Err(format!("a list index is a number, got {}", type_display(&index_t)));
                    }
                    // VAPP-204: an index may be past the end (null).
                    return Ok(optional_type(item.first().cloned().unwrap_or_else(|| ty("dyn"))));
                }
                if is_row(&of_t) || is(&of_t, "map") {
                    if !loose(&index_t) && !is_textual(&index_t) {
                        return Err(format!("a map key is a string, got {}", type_display(&index_t)));
                    }
                    if let (Value::Object(fields), Some(Value::String(key))) = (&of_t, literal(index)) {
                        return fields.get(key).cloned().map(wrap).ok_or_else(|| format!("unknown field {key}"));
                    }
                    return Ok(optional_type(ty("dyn")));
                }
                if loose(&of_t) {
                    return Ok(wrap(ty("dyn")));
                }
                Err(format!("cannot index {}", type_display(&of_t)))
            }
            Node::Unary(op, arg, _) => {
                let t = self.need(arg, scope)?;
                let want = if *op == '-' { "number" } else { "bool" };
                if !loose(&t) && !is(&t, want) {
                    return Err(format!("cannot apply {op} to {}", type_display(&t)));
                }
                Ok(ty(want))
            }
            Node::Cond(test, then, otherwise, _) => {
                let t = self.need(test, scope)?;
                if !loose(&t) && !is(&t, "bool") {
                    return Err(format!("cannot use {} as a condition", type_display(&t)));
                }
                let (yes, no) = guards(test);
                let a = self.within(yes, then, scope)?;
                let b = self.within(no, otherwise, scope)?;
                Ok(if a == b {
                    a
                } else if is(&a, "null") {
                    optional_type(b)
                } else if is(&b, "null") {
                    optional_type(a)
                } else if required_type(&a) == required_type(&b) {
                    optional_type(required_type(&a))
                } else if is_optional(&a) || is_optional(&b) {
                    optional_type(ty("dyn"))
                } else {
                    ty("dyn")
                })
            }
            Node::Bin(op, a, b, _) => self.binary(op, a, b, scope),
            Node::Call(name, args, _) => self.call(name, args, scope),
            Node::Macro(f, of, var, body, _) => {
                let of = self.need(of, scope)?;
                if !of.is_array() && !loose(&of) {
                    return Err(format!("{f} needs a list, got {}", type_display(&of)));
                }
                let item = of.as_array().and_then(|a| a.first().cloned()).unwrap_or_else(|| ty("dyn"));
                let inner = TypeScope { name: var, ty: item, parent: scope };
                let body_t = self.tc(body, Some(&inner))?;
                if f == "map" {
                    return Ok(Value::Array(vec![body_t]));
                }
                if is_optional(&body_t) {
                    return missing(body);
                }
                if !loose(&body_t) && !is(&body_t, "bool") {
                    return Err(format!("{f} needs a bool, got {}", type_display(&body_t)));
                }
                Ok(if f == "filter" {
                    if of.is_array() {
                        of
                    } else {
                        Value::Array(vec![ty("dyn")])
                    }
                } else {
                    ty("bool")
                })
            }
        }
    }

    fn binary(&self, op: &str, a: &Node, b: &Node, scope: Option<&TypeScope>) -> R<Value> {
        if op == "&&" || op == "||" {
            let x = self.need(a, scope)?;
            let (yes, no) = guards(a);
            let y = self.within(if op == "&&" { yes } else { no }, b, scope)?;
            if is_optional(&y) {
                return missing(b);
            }
            for t in [&x, &y] {
                if !loose(t) && !is(t, "bool") {
                    return Err(format!("cannot apply {op} to {}", type_display(t)));
                }
            }
            return Ok(ty("bool"));
        }
        if op == "==" || op == "!=" {
            // The null check: either side may be missing.
            let x = required_type(&self.tc(a, scope)?);
            let y = required_type(&self.tc(b, scope)?);
            let (dx, dy) = (type_display(&x), type_display(&y));
            let comparable = loose(&x) || loose(&y) || (is_textual(&x) && is_textual(&y)) || dx == dy;
            if !comparable {
                return Err(format!("cannot compare {dx} and {dy}"));
            }
            return Ok(ty("bool"));
        }
        let x = self.need(a, scope)?;
        let y = self.need(b, scope)?;
        let (dx, dy) = (type_display(&x), type_display(&y));
        let numeric = |t: &Value| is(t, "number") || loose(t);
        match op {
            "<" | "<=" | ">" | ">=" => {
                let ok = |t: &Value| loose(t) || is(t, "number") || is_textual(t);
                if !ok(&x) || !ok(&y) || (is(&x, "number") && is_textual(&y)) || (is(&y, "number") && is_textual(&x)) {
                    return Err(format!("cannot compare {dx} and {dy}"));
                }
                Ok(ty("bool"))
            }
            "+" => {
                if loose(&x) && loose(&y) {
                    return Ok(ty("dyn"));
                }
                if numeric(&x) && numeric(&y) {
                    return Ok(ty("number"));
                }
                if (is_textual(&x) || loose(&x)) && (is_textual(&y) || loose(&y)) {
                    return Ok(ty("string"));
                }
                if (x.is_array() || loose(&x)) && (y.is_array() || loose(&y)) {
                    return Ok(if x.is_array() && y.is_array() && x == y { x } else { Value::Array(vec![ty("dyn")]) });
                }
                Err(format!("cannot apply + to {dx} and {dy}"))
            }
            _ => {
                if !(numeric(&x) && numeric(&y)) {
                    return Err(format!("cannot apply {op} to {dx} and {dy}"));
                }
                if (op == "/" || op == "%") && literal(b).is_some_and(|v| v.as_f64() == Some(0.0)) {
                    return Err(if op == "/" { "division by zero" } else { "modulo by zero" }.into());
                }
                Ok(ty("number"))
            }
        }
    }

    fn call(&self, name: &str, args: &[Node], scope: Option<&TypeScope>) -> R<Value> {
        let d = def(name).ok_or_else(|| format!("unknown function {name}"))?;
        if let Some(e) = arity_error(name, &d, args.len()) {
            return Err(e);
        }
        // orElse takes a value that may be missing; every other argument is used.
        let types = args.iter().enumerate().map(|(i, a)| if name == "orElse" && i == 0 { self.tc(a, scope) } else { self.need(a, scope) }).collect::<R<Vec<_>>>()?;
        for (i, t) in types.iter().enumerate() {
            let param = d.params[i.min(d.params.len() - 1)];
            if !accepts(param, &required_type(t)) {
                return Err(format!("{name} needs {}, got {}", param_name(param), type_display(t)));
            }
        }
        if name == "orElse" {
            let (x, y) = (required_type(&types[0]), &types[1]);
            if !loose(&x) && !loose(y) && !(is_textual(&x) && is_textual(y)) && type_display(&x) != type_display(y) {
                return Err(format!("orElse needs a {} default, got {}", type_display(&x), type_display(y)));
            }
        }
        let lit = |i: usize| args.get(i).and_then(literal);
        let unit_at = match name {
            "dateAdd" | "dateDiff" => Some(2),
            "startOf" | "endOf" => Some(1),
            _ => None,
        };
        if let Some(Value::String(unit)) = unit_at.and_then(lit) {
            let units = if unit_at == Some(2) { DATE_UNITS } else { CALENDAR_UNITS };
            if !units.contains(&unit.as_str()) {
                return Err(format!("unknown unit {unit}"));
            }
            if name == "dateAdd" && is(&types[0], "date") && unit_ms(unit).is_some() {
                return Err(format!("unit {unit} needs a timestamp"));
            }
        }
        if name == "dateAdd" {
            if let Some(n) = lit(1).and_then(Value::as_f64) {
                if n.fract() != 0.0 {
                    return Err(format!("dateAdd needs a whole number, got {}", num_text(n)));
                }
            }
        }
        if name == "round" && args.len() > 1 {
            if let Some(d) = lit(1).and_then(Value::as_f64) {
                if d.fract() != 0.0 || !(0.0..=10.0).contains(&d) {
                    return Err("round needs 0 to 10 digits".into());
                }
            }
        }
        if name == "sortBy" {
            if let Some(Value::String(o)) = lit(2) {
                if o != "asc" && o != "desc" {
                    return Err("sortBy order is asc or desc".into());
                }
            }
        }
        if name == "split" && matches!(lit(1), Some(Value::String(s)) if s.is_empty()) {
            return Err("split needs a separator".into());
        }
        if name == "div" && lit(1).and_then(Value::as_f64) == Some(0.0) {
            return Err("division by zero".into());
        }
        if name == "mod" && lit(1).and_then(Value::as_f64) == Some(0.0) {
            return Err("modulo by zero".into());
        }
        if name == "formatDuration" {
            if let Some(Value::String(f)) = lit(1) {
                if !DURATION_FORMATS.contains(&f.as_str()) {
                    return Err(format!("unknown duration format {f}"));
                }
            }
        }
        Ok(match name {
            "dateAdd" | "startOf" | "endOf" => {
                if is(&types[0], "date") || is(&types[0], "timestamp") {
                    types[0].clone()
                } else {
                    ty("dyn")
                }
            }
            "today" => ty("date"),
            "now" | "dateTime" => ty("timestamp"),
            "formatDuration" | "string" | "trim" | "lower" | "upper" | "join" => ty("string"),
            "split" => Value::Array(vec![ty("string")]),
            "avgOf" | "minOf" | "maxOf" => optional_type(ty("number")),
            "sortBy" => {
                if types[0].is_array() {
                    types[0].clone()
                } else {
                    Value::Array(vec![ty("dyn")])
                }
            }
            "groupOf" => {
                let rows = if types[0].is_array() { types[0].clone() } else { Value::Array(vec![ty("dyn")]) };
                Value::Array(vec![serde_json::json!({"key": optional_type(ty("dyn")), "rows": rows})])
            }
            "orElse" => {
                let x = required_type(&types[0]);
                if is(&x, "null") {
                    required_type(&types[1])
                } else {
                    x
                }
            }
            _ => ty("number"),
        })
    }
}

/// Type-check an expression against the names it may read (a kind's
/// fields, a binding's value): the type, or the error (the parse error, or
/// the first type error). Types are spelled as JSON (`"number"`, `[T]`, a
/// row object, `{"?": T}` for a value that may be missing).
pub fn check_expression(source: &str, types: &Map<String, Value>) -> Result<Value, String> {
    let ast = parse_expression(source)?;
    Checker { types, present: std::cell::RefCell::new(Vec::new()) }.tc(&ast, None)
}
