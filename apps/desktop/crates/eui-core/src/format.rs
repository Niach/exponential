//! Round 2 (docs/round-2-contract.md §3): FORMATTING through a host-supplied
//! [`Formatter`]. Every number, currency, percent, date and relative time a
//! surface shows (Table cells, NumberField, chart ticks, calendar names, the
//! `format*` bind functions) goes through ONE formatter per surface, in the
//! surface's `locale` and the host's time zone. The default is
//! [`EnglishFormatter`] (`catalog/locale.json` `englishFallback`, = Intl
//! en-US, instants in UTC); [`ZonedEnglishFormatter`] = the same at the
//! host's UTC offset (the core has no zone database: the zone reaches it
//! ONLY through the formatter); an FFI host passes its own (Foundation,
//! `android.icu`).
//!
//! The core does the PARSING and the DECISIONS (what a date value names,
//! which relative-time unit, which plural arm) so every platform agrees;
//! a [`Formatter`] only localizes the result. Mirrors `src/format.ts`;
//! `fixtures/format.json` locks it.

use std::sync::LazyLock;

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::generated::catalog as g;
use crate::json;

/// A CLDR plural category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<PluralCategory> {
        Some(match s {
            "zero" => PluralCategory::Zero,
            "one" => PluralCategory::One,
            "two" => PluralCategory::Two,
            "few" => PluralCategory::Few,
            "many" => PluralCategory::Many,
            "other" => PluralCategory::Other,
            _ => return None,
        })
    }
}

/// A date preset (`style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateStyle {
    Short,
    #[default]
    Medium,
    Long,
    Full,
}

impl DateStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            DateStyle::Short => "short",
            DateStyle::Medium => "medium",
            DateStyle::Long => "long",
            DateStyle::Full => "full",
        }
    }

    pub fn parse(s: &str) -> Option<DateStyle> {
        Some(match s {
            "short" => DateStyle::Short,
            "medium" => DateStyle::Medium,
            "long" => DateStyle::Long,
            "full" => DateStyle::Full,
            _ => return None,
        })
    }
}

/// The unit a relative time is shown in ([`relative_time_unit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl RelativeUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            RelativeUnit::Second => "second",
            RelativeUnit::Minute => "minute",
            RelativeUnit::Hour => "hour",
            RelativeUnit::Day => "day",
            RelativeUnit::Week => "week",
            RelativeUnit::Month => "month",
            RelativeUnit::Year => "year",
        }
    }

    pub fn parse(s: &str) -> Option<RelativeUnit> {
        Some(match s {
            "second" => RelativeUnit::Second,
            "minute" => RelativeUnit::Minute,
            "hour" => RelativeUnit::Hour,
            "day" => RelativeUnit::Day,
            "week" => RelativeUnit::Week,
            "month" => RelativeUnit::Month,
            "year" => RelativeUnit::Year,
            _ => return None,
        })
    }
}

/// Number options: `decimals` = fixed fraction digits (min = max; `None` =
/// 0..3 for numbers, the currency's minor digits, 0 for percents);
/// `grouping` = locale separators (default true).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberOptions {
    pub decimals: Option<u32>,
    pub grouping: bool,
}

impl Default for NumberOptions {
    fn default() -> Self {
        NumberOptions { decimals: None, grouping: true }
    }
}

/// Date options: `format` (a TR35 pattern, wins) or `style` (default
/// medium) + `time` (append the short time).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DateOptions {
    pub format: Option<String>,
    pub style: Option<DateStyle>,
    pub time: bool,
}

/// The instant a date value names ([`parse_date_value`]): epoch ms, and
/// whether it was a calendar day (`yyyy-mm-dd`, UTC midnight: format it in
/// UTC, never shifted into the surface zone).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateValue {
    pub ms: f64,
    pub date_only: bool,
}

/// What a host supplies per surface (in `SurfaceSettings.locale` and its
/// own time zone). The core parses date values and picks the relative unit;
/// the formatter localizes. [`EnglishFormatter`] is the default.
pub trait Formatter: Send + Sync {
    /// BCP 47.
    fn locale(&self) -> String;
    fn number(&self, value: f64, options: NumberOptions) -> String;
    /// `code` = an upper-case ISO 4217 code.
    fn currency(&self, value: f64, code: &str, options: NumberOptions) -> String;
    /// `value` is a ratio: 0.256 → `26%`.
    fn percent(&self, value: f64, decimals: Option<u32>) -> String;
    fn date(&self, value: DateValue, options: &DateOptions) -> String;
    /// `value` units relative to now (negative = past), truncated by the core.
    fn relative_time(&self, value: i64, unit: RelativeUnit) -> String;
    fn plural(&self, value: f64) -> PluralCategory;
    /// A byte size (`value` already in `unit`, ≤ 1 fraction digit), like
    /// `Intl.NumberFormat(locale, {style: "unit", unitDisplay: "short"})`.
    /// The default: the number through [`Self::number`] + the English
    /// short unit (`byte`, `kB`, `MB`, `GB`).
    fn bytes(&self, value: f64, unit: ByteUnit) -> String {
        let decimals = if value.fract() == 0.0 { 0 } else { 1 };
        format!("{} {}", self.number(value, NumberOptions { decimals: Some(decimals), grouping: true }), unit.english())
    }
}

/// A byte-size unit ([`Formatter::bytes`]; the CLDR unit ids).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteUnit {
    Byte,
    Kilobyte,
    Megabyte,
    Gigabyte,
}

impl ByteUnit {
    /// The CLDR unit id (`byte`, `kilobyte`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            ByteUnit::Byte => "byte",
            ByteUnit::Kilobyte => "kilobyte",
            ByteUnit::Megabyte => "megabyte",
            ByteUnit::Gigabyte => "gigabyte",
        }
    }

    /// The en-US short form.
    pub fn english(self) -> &'static str {
        match self {
            ByteUnit::Byte => "byte",
            ByteUnit::Kilobyte => "kB",
            ByteUnit::Megabyte => "MB",
            ByteUnit::Gigabyte => "GB",
        }
    }
}

// ---------------------------------------------------------------------------
// The English fallback data (catalog/locale.json `englishFallback`)
// ---------------------------------------------------------------------------

/// Month, weekday and day-period names a pattern prints.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateNames {
    pub months: Vec<String>,
    pub months_short: Vec<String>,
    pub weekdays: Vec<String>,
    pub weekdays_short: Vec<String>,
    pub day_periods: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelativeData {
    units: IndexMap<String, Vec<String>>,
    auto: IndexMap<String, IndexMap<String, String>>,
    past: String,
    future: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fallback {
    #[serde(flatten)]
    names: DateNames,
    currency_prefix: IndexMap<String, String>,
    currency_digits: IndexMap<String, u32>,
    relative: RelativeData,
    unknown_currency: String,
    date_patterns: IndexMap<String, String>,
    time_pattern: String,
    date_time_join: IndexMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocaleFile {
    english_fallback: Fallback,
}

static FALLBACK: LazyLock<Fallback> = LazyLock::new(|| serde_json::from_str::<LocaleFile>(g::LOCALE_JSON).expect("locale.json englishFallback").english_fallback);

/// The English names (`englishFallback`).
pub fn english_names() -> &'static DateNames {
    &FALLBACK.names
}

/// The en-US preset patterns (`style`) and the time a preset appends.
pub const ENGLISH_DATE_PATTERNS: [(DateStyle, &str); 4] = [(DateStyle::Short, "M/d/yy"), (DateStyle::Medium, "MMM d, y"), (DateStyle::Long, "MMMM d, y"), (DateStyle::Full, "EEEE, MMMM d, y")];
pub const ENGLISH_TIME_PATTERN: &str = "h:mm a";

/// The minor digits of a currency (ISO 4217; 2 when not listed).
pub fn currency_digits(code: &str) -> u32 {
    FALLBACK.currency_digits.get(&code.to_uppercase()).copied().unwrap_or(2)
}

// ---------------------------------------------------------------------------
// Display of a non-string value in a text prop
// ---------------------------------------------------------------------------

/// What a `string`/`markdown` prop SHOWS for a bound value: strings as they
/// are, finite numbers as ECMAScript `Number.prototype.toString` (`412`,
/// `1.5`, `1e-7`, `1e+21`, `-0` → `0`), booleans `true`/`false`; null,
/// non-finite numbers, objects and arrays show nothing. No locale (use
/// `formatNumber`). `src/format.ts displayString`.
pub fn display_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.is_finite() => json::number_to_string(f),
            _ => String::new(),
        },
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// [`display_string`] of an optional value (absent = ``).
pub fn display_opt(value: Option<&Value>) -> String {
    value.map(display_string).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Decimal arithmetic on the shortest round-trip string (ICU's halfExpand)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Decimal {
    int: String,
    frac: String,
}

/// The shortest round-trip digits of a finite `abs` ≥ 0 and the position of
/// the decimal point (`digits = "125", point = 1` = 1.25).
pub(crate) fn shortest_digits(abs: f64) -> (String, i32) {
    if abs == 0.0 {
        return ("0".into(), 1);
    }
    let e = format!("{abs:e}");
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exp: i32 = exp.parse().unwrap_or(0);
    (digits, exp + 1)
}

fn decimal_of(abs: f64) -> Decimal {
    let (digits, point) = shortest_digits(abs);
    let mut all = digits;
    let mut point = point;
    if point <= 0 {
        all = "0".repeat((1 - point) as usize) + &all;
        point = 1;
    }
    let point = point as usize;
    if point > all.len() {
        all.push_str(&"0".repeat(point - all.len()));
    }
    let int = all[..point].trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    Decimal { int: int.to_string(), frac: all[point..].trim_end_matches('0').to_string() }
}

/// Move the point `places` to the right (percent = 2).
fn shift(d: &Decimal, places: usize) -> Decimal {
    let mut frac = d.frac.clone();
    if frac.len() < places {
        frac.push_str(&"0".repeat(places - frac.len()));
    }
    let int = format!("{}{}", d.int, &frac[..places]);
    let int = int.trim_start_matches('0');
    Decimal { int: if int.is_empty() { "0".into() } else { int.to_string() }, frac: frac[places..].trim_end_matches('0').to_string() }
}

/// Round to at most `max` fraction digits, half away from zero, then pad
/// to `min`.
fn round(d: &Decimal, min: usize, max: usize) -> Decimal {
    let mut int = d.int.clone();
    let mut frac = d.frac.clone();
    if frac.len() > max {
        let up = frac.as_bytes()[max] >= b'5';
        let mut kept: Vec<u8> = format!("{int}{}", &frac[..max]).into_bytes();
        if up {
            let mut i = kept.len();
            let mut carried = true;
            while i > 0 {
                i -= 1;
                if kept[i] == b'9' {
                    kept[i] = b'0';
                } else {
                    kept[i] += 1;
                    carried = false;
                    break;
                }
            }
            if carried {
                kept.insert(0, b'1');
            }
        }
        let kept = String::from_utf8(kept).unwrap_or_default();
        let int_len = kept.len() - max;
        int = if int_len == 0 { "0".into() } else { kept[..int_len].to_string() };
        frac = kept[int_len..].trim_end_matches('0').to_string();
    }
    if frac.len() < min {
        frac.push_str(&"0".repeat(min - frac.len()));
    }
    Decimal { int, frac }
}

fn group(int: &str, on: bool) -> String {
    if !on || int.len() < 4 {
        return int.to_string();
    }
    let mut out = String::with_capacity(int.len() + int.len() / 3);
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn english_number(value: f64, min: usize, max: usize, grouping: bool) -> (&'static str, String) {
    let neg = value < 0.0 || (value == 0.0 && value.is_sign_negative());
    if value.is_nan() {
        return ("", "NaN".into());
    }
    if value.is_infinite() {
        return (if neg { "-" } else { "" }, "∞".into());
    }
    let d = round(&decimal_of(value.abs()), min, max);
    let body = if d.frac.is_empty() { group(&d.int, grouping) } else { format!("{}.{}", group(&d.int, grouping), d.frac) };
    (if neg { "-" } else { "" }, body)
}

/// `decimals` as fixed fraction digits: finite → trunc, clamped to 0..20.
pub fn fixed_digits(decimals: f64) -> Option<u32> {
    decimals.is_finite().then(|| decimals.trunc().clamp(0.0, 20.0) as u32)
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

const MS_SECOND: f64 = 1_000.0;
const MS_MINUTE: f64 = 60_000.0;
const MS_HOUR: f64 = 3_600_000.0;
const MS_DAY: f64 = 86_400_000.0;

/// Days since 1970-01-01 of a proleptic Gregorian date (month 1..12).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month 1..12, day) of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// JS `Date.UTC(y, m - 1, d, h, mi, s, ms)`: out-of-range months and days
/// roll over.
fn date_utc(y: i64, m: i64, d: i64, h: i64, mi: i64, s: i64, ms: f64) -> f64 {
    let y = y + (m - 1).div_euclid(12);
    let m = (m - 1).rem_euclid(12) + 1;
    let days = days_from_civil(y, m, 1) + (d - 1);
    days as f64 * MS_DAY + (h as f64) * MS_HOUR + (mi as f64) * MS_MINUTE + (s as f64) * MS_SECOND + ms
}

fn digits_n(s: &str, n: usize) -> Option<i64> {
    (s.len() == n && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten()
}

/// The instant a date value names: `yyyy-mm-dd` = that calendar day (UTC
/// midnight, `date_only`), an ISO date-time `yyyy-mm-ddThh:mm[:ss[.f]][Z|±hh:mm]`
/// (no offset = UTC), or epoch milliseconds; anything else = `None`.
pub fn parse_date_value(value: &Value) -> Option<DateValue> {
    match value {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map(|ms| DateValue { ms, date_only: false }),
        Value::String(s) => parse_date_str(s),
        _ => None,
    }
}

fn parse_date_str(s: &str) -> Option<DateValue> {
    if s.trim().is_empty() || !s.is_ascii() {
        return None;
    }
    let ymd = |d: &str| -> Option<(i64, i64, i64)> {
        let mut it = d.split('-');
        let y = digits_n(it.next()?, 4)?;
        let m = digits_n(it.next()?, 2)?;
        let day = digits_n(it.next()?, 2)?;
        it.next().is_none().then_some((y, m, day))
    };
    if s.len() == 10 {
        let (y, m, d) = ymd(s)?;
        return Some(DateValue { ms: date_utc(y, m, d, 0, 0, 0, 0.0), date_only: true });
    }
    let (date, rest) = s.split_once('T')?;
    let (y, m, d) = ymd(date)?;
    // The zone suffix.
    let (time, offset_min) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0i64)
    } else if rest.len() > 6 && matches!(rest.as_bytes()[rest.len() - 6], b'+' | b'-') {
        let (t, z) = rest.split_at(rest.len() - 6);
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let (hh, mm) = z[1..].split_once(':')?;
        let (hh, mm) = (digits_n(hh, 2)?, digits_n(mm, 2)?);
        if hh > 23 || mm > 59 {
            return None;
        }
        (t, sign * (hh * 60 + mm))
    } else {
        (rest, 0)
    };
    let mut parts = time.split(':');
    let h = digits_n(parts.next()?, 2)?;
    let mi = digits_n(parts.next()?, 2)?;
    let (sec, frac_ms) = match parts.next() {
        None => (0, 0.0),
        Some(sec) => {
            let (whole, frac) = match sec.split_once('.') {
                Some((w, f)) => {
                    if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    (w, f)
                }
                None => (sec, ""),
            };
            let whole = digits_n(whole, 2)?;
            // Milliseconds: the first three fraction digits (JS truncates).
            let ms: f64 = if frac.is_empty() { 0.0 } else { format!("{:0<3}", &frac[..frac.len().min(3)]).parse().unwrap_or(0.0) };
            (whole, ms)
        }
    };
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) || mi > 59 || sec > 59 || h > 24 || (h == 24 && (mi > 0 || sec > 0 || frac_ms > 0.0)) {
        return None;
    }
    let ms = date_utc(y, m, d, h, mi, sec, frac_ms) - (offset_min as f64) * MS_MINUTE;
    ms.is_finite().then_some(DateValue { ms, date_only: false })
}

/// The calendar fields of an instant (in UTC; a host shifts by its zone first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateFields {
    pub year: i64,
    /// 1..12
    pub month: u32,
    pub day: u32,
    /// 0 = Sunday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// The UTC fields of epoch `ms`.
pub fn utc_fields(ms: f64) -> DateFields {
    let days = (ms / MS_DAY).floor() as i64;
    let rem = (ms - days as f64 * MS_DAY).max(0.0);
    let (year, month, day) = civil_from_days(days);
    let secs = (rem / MS_SECOND).floor() as i64;
    DateFields {
        year,
        month,
        day,
        weekday: (days + 4).rem_euclid(7) as u32,
        hour: (secs / 3600) as u32,
        minute: ((secs / 60) % 60) as u32,
        second: (secs % 60) as u32,
    }
}

fn pad(n: i64, width: usize) -> String {
    if n < 0 {
        return format!("-{:0>width$}", -n, width = width);
    }
    format!("{n:0>width$}")
}

/// A TR35 pattern over fields: `y yy yyyy M MM MMM MMMM d dd E EEE EEEE h hh
/// H HH m mm s ss a`, `'quoted'` literals (`''` = an apostrophe); any other
/// letter run is copied as written. Digits are ASCII.
pub fn format_pattern(pattern: &str, f: &DateFields, names: &DateNames) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let name = |list: &Vec<String>, i: usize| list.get(i).cloned().unwrap_or_default();
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        out.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if !ch.is_ascii_alphabetic() {
            out.push(ch);
            i += 1;
            continue;
        }
        let mut n = 1;
        while chars.get(i + n) == Some(&ch) {
            n += 1;
        }
        i += n;
        let h12 = if f.hour.is_multiple_of(12) { 12 } else { f.hour % 12 };
        match ch {
            'y' => out.push_str(&if n == 2 { pad(f.year.rem_euclid(100), 2) } else { pad(f.year, n) }),
            'M' => out.push_str(&match n {
                4.. => name(&names.months, f.month as usize - 1),
                3 => name(&names.months_short, f.month as usize - 1),
                _ => pad(f.month as i64, n),
            }),
            'd' => out.push_str(&pad(f.day as i64, n)),
            'E' => out.push_str(&if n >= 4 { name(&names.weekdays, f.weekday as usize) } else { name(&names.weekdays_short, f.weekday as usize) }),
            'h' => out.push_str(&pad(h12 as i64, n)),
            'H' => out.push_str(&pad(f.hour as i64, n)),
            'm' => out.push_str(&pad(f.minute as i64, n)),
            's' => out.push_str(&pad(f.second as i64, n)),
            'a' => out.push_str(&name(&names.day_periods, if f.hour < 12 { 0 } else { 1 })),
            _ => (0..n).for_each(|_| out.push(ch)),
        }
    }
    out
}

/// The unit a delta (value − now, ms) shows in and its count (truncated
/// toward zero): under a minute seconds, under an hour minutes, under a
/// day hours, under 7 days days, under 30 days weeks, under 365 days
/// months (of 30 days), else years (of 365 days).
pub fn relative_time_unit(delta_ms: f64) -> (i64, RelativeUnit) {
    let abs = delta_ms.abs();
    let t = |unit: f64| {
        let v = (delta_ms / unit).trunc();
        if v == 0.0 {
            0
        } else {
            v as i64
        }
    };
    if abs < MS_MINUTE {
        (t(MS_SECOND), RelativeUnit::Second)
    } else if abs < MS_HOUR {
        (t(MS_MINUTE), RelativeUnit::Minute)
    } else if abs < MS_DAY {
        (t(MS_HOUR), RelativeUnit::Hour)
    } else if abs < 7.0 * MS_DAY {
        (t(MS_DAY), RelativeUnit::Day)
    } else if abs < 30.0 * MS_DAY {
        (t(7.0 * MS_DAY), RelativeUnit::Week)
    } else if abs < 365.0 * MS_DAY {
        (t(30.0 * MS_DAY), RelativeUnit::Month)
    } else {
        (t(365.0 * MS_DAY), RelativeUnit::Year)
    }
}

// ---------------------------------------------------------------------------
// The English fallback
// ---------------------------------------------------------------------------

/// The formatter a surface uses without a host one: Intl en-US decided by
/// data (`englishFallback`), instants in UTC. Equal to Intl en-US on every
/// case of `fixtures/format.json`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnglishFormatter;

impl Formatter for EnglishFormatter {
    fn locale(&self) -> String {
        "en-US".into()
    }

    fn number(&self, value: f64, options: NumberOptions) -> String {
        let (min, max) = options.decimals.map(|d| (d as usize, d as usize)).unwrap_or((0, 3));
        let (sign, body) = english_number(value, min, max, options.grouping);
        format!("{sign}{body}")
    }

    fn currency(&self, value: f64, code: &str, options: NumberOptions) -> String {
        let code = code.to_uppercase();
        let digits = options.decimals.unwrap_or_else(|| currency_digits(&code)) as usize;
        let (sign, body) = english_number(value, digits, digits, options.grouping);
        let prefix = FALLBACK.currency_prefix.get(&code).cloned().unwrap_or_else(|| FALLBACK.unknown_currency.replace("{code}", &code));
        format!("{sign}{prefix}{body}")
    }

    fn percent(&self, value: f64, decimals: Option<u32>) -> String {
        let digits = decimals.unwrap_or(0) as usize;
        if !value.is_finite() {
            let (sign, body) = english_number(value, 0, 0, true);
            return format!("{sign}{body}%");
        }
        let neg = value < 0.0 || (value == 0.0 && value.is_sign_negative());
        let d = round(&shift(&decimal_of(value.abs()), 2), digits, digits);
        let body = if d.frac.is_empty() { group(&d.int, true) } else { format!("{}.{}", group(&d.int, true), d.frac) };
        format!("{}{body}%", if neg { "-" } else { "" })
    }

    fn date(&self, value: DateValue, options: &DateOptions) -> String {
        english_date(value, options, 0)
    }

    fn relative_time(&self, value: i64, unit: RelativeUnit) -> String {
        if let Some(auto) = FALLBACK.relative.auto.get(unit.as_str()).and_then(|m| m.get(&value.to_string())) {
            return auto.clone();
        }
        let names = &FALLBACK.relative.units[unit.as_str()];
        let count = english_number(value.unsigned_abs() as f64, 0, 0, true).1;
        let unit_name = if value.unsigned_abs() == 1 { &names[0] } else { &names[1] };
        let template = if value < 0 { &FALLBACK.relative.past } else { &FALLBACK.relative.future };
        template.replace("{value}", &count).replace("{unit}", unit_name)
    }

    fn plural(&self, value: f64) -> PluralCategory {
        if value.abs() == 1.0 {
            PluralCategory::One
        } else {
            PluralCategory::Other
        }
    }
}

/// The English fallback's date at `offset_minutes` east of UTC (a calendar
/// day never shifts): the patterns and joiners are `englishFallback` data.
fn english_date(value: DateValue, options: &DateOptions, offset_minutes: i32) -> String {
    let ms = if value.date_only { value.ms } else { value.ms + offset_minutes as f64 * 60_000.0 };
    let f = utc_fields(ms);
    let names = english_names();
    if let Some(pattern) = &options.format {
        return format_pattern(pattern, &f, names);
    }
    let style = options.style.unwrap_or_default();
    let pattern = FALLBACK.date_patterns.get(style.as_str()).map(String::as_str).unwrap_or("MMM d, y");
    let date = format_pattern(pattern, &f, names);
    if !options.time {
        return date;
    }
    let time = format_pattern(&FALLBACK.time_pattern, &f, names);
    let join = FALLBACK.date_time_join.get(style.as_str()).map(String::as_str).unwrap_or("{date}, {time}");
    join.replace("{date}", &date).replace("{time}", &time)
}

/// The surface zone as the English fallback sees it: the UTC offset in
/// minutes (east positive) in force at an instant (epoch ms). The core has
/// no zone database: the host passes its platform's offset (gpui: the
/// local zone; the fixtures pin a fixed one).
pub type ZoneOffset = std::sync::Arc<dyn Fn(f64) -> i32 + Send + Sync>;

/// [`EnglishFormatter`] in a surface zone: instants shift by the host's
/// [`ZoneOffset`], a `yyyy-mm-dd` day never does (`englishFormatter(zone)`).
#[derive(Clone)]
pub struct ZonedEnglishFormatter {
    offset: ZoneOffset,
}

impl ZonedEnglishFormatter {
    pub fn new(offset: ZoneOffset) -> Self {
        ZonedEnglishFormatter { offset }
    }

    /// A fixed offset (minutes east of UTC); `fixed(0)` = [`EnglishFormatter`].
    pub fn fixed(minutes: i32) -> Self {
        ZonedEnglishFormatter { offset: std::sync::Arc::new(move |_| minutes) }
    }
}

impl std::fmt::Debug for ZonedEnglishFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZonedEnglishFormatter").finish_non_exhaustive()
    }
}

impl Formatter for ZonedEnglishFormatter {
    fn locale(&self) -> String {
        ENGLISH.locale()
    }
    fn number(&self, value: f64, options: NumberOptions) -> String {
        ENGLISH.number(value, options)
    }
    fn currency(&self, value: f64, code: &str, options: NumberOptions) -> String {
        ENGLISH.currency(value, code, options)
    }
    fn percent(&self, value: f64, decimals: Option<u32>) -> String {
        ENGLISH.percent(value, decimals)
    }
    fn date(&self, value: DateValue, options: &DateOptions) -> String {
        english_date(value, options, (self.offset)(value.ms))
    }
    fn relative_time(&self, value: i64, unit: RelativeUnit) -> String {
        ENGLISH.relative_time(value, unit)
    }
    fn plural(&self, value: f64) -> PluralCategory {
        ENGLISH.plural(value)
    }
}

/// The shared English formatter.
pub static ENGLISH: EnglishFormatter = EnglishFormatter;

/// Typed number text in the formatter's own separators and digits (the
/// reference NumberField's `parseLocaleNumber`): the digits, group and
/// decimal separator are read off `formatter.number(…)`; group separators,
/// spaces and bidi marks drop, the decimal becomes `.`, `−`/`–` become `-`.
/// `None` when empty or unreadable.
pub fn parse_number(formatter: &dyn Formatter, text: &str) -> Option<f64> {
    // The locale's digits: 9876543210 printed without grouping.
    let digits: Vec<char> = formatter.number(9_876_543_210.0, NumberOptions { decimals: Some(0), grouping: false }).chars().filter(|c| c.is_numeric()).collect();
    let digit_of = |c: char| -> Option<char> {
        if digits.len() == 10 {
            digits.iter().position(|d| *d == c).map(|i| char::from(b'9' - i as u8))
        } else {
            None
        }
    };
    // The separators: the runs between the digits of 12345.6 (`12,345.6`).
    let sample = formatter.number(12_345.6, NumberOptions { decimals: Some(1), grouping: true });
    // Only runs BETWEEN digits count (never a prefix or a suffix).
    let mut seps: Vec<String> = Vec::new();
    let mut run = String::new();
    let mut seen_digit = false;
    for c in sample.chars() {
        if c.is_numeric() {
            if !run.is_empty() {
                seps.push(std::mem::take(&mut run));
            }
            seen_digit = true;
        } else if seen_digit {
            run.push(c);
        }
    }
    let (group, decimal) = match seps.as_slice() {
        [d] => (None, d.trim().to_string()),
        [g, .., d] => (Some(g.clone()), d.trim().to_string()),
        [] => (None, String::new()),
    };
    let decimal = if decimal.is_empty() { ".".to_string() } else { decimal };
    let ascii: String = text.chars().map(|c| if c.is_ascii_digit() { c } else { digit_of(c).unwrap_or(c) }).collect();
    let mut clean = ascii.trim().to_string();
    if let Some(g) = group.as_deref().filter(|g| !g.trim().is_empty()) {
        clean = clean.replace(g, "");
    }
    clean.retain(|c| !c.is_whitespace() && !matches!(c, '\u{200e}' | '\u{200f}' | '\u{61c}'));
    let clean = clean.replacen(&decimal, ".", 1).replace(['−', '–'], "-");
    if clean.is_empty() || clean == "-" {
        return None;
    }
    clean.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A date value formatted through `formatter` (`` when unreadable).
pub fn format_date_value(formatter: &dyn Formatter, value: &Value, options: &DateOptions) -> String {
    parse_date_value(value).map(|v| formatter.date(v, options)).unwrap_or_default()
}

/// A relative time of `value` against `now_ms` (`` when unreadable).
pub fn format_relative_value(formatter: &dyn Formatter, value: &Value, now_ms: f64) -> String {
    match parse_date_value(value) {
        Some(v) => {
            let (n, unit) = relative_time_unit(v.ms - now_ms);
            formatter.relative_time(n, unit)
        }
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// The format functions of the bind table
// ---------------------------------------------------------------------------

/// The bind-time format functions (A2UI basic `formatNumber`,
/// `formatCurrency`, `formatDate`, `pluralize` + core `formatPercent`,
/// `formatRelativeTime`), all through ONE formatter.
pub const FORMAT_FUNCTION_NAMES: &[&str] = g::FORMAT_FUNCTION_NAMES;

pub fn is_format_function(name: &str) -> bool {
    FORMAT_FUNCTION_NAMES.contains(&name)
}

/// The wall clock in epoch ms.
pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// JS `typeof v === "number" ? v : typeof v === "string" && v.trim() !== "" ? Number(v) : NaN`.
fn to_num(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_number_from_str(s),
        _ => f64::NAN,
    }
}

/// A numeric string (`[ws][+-](digits[.digits]|.digits)[e[+-]digits][ws]`,
/// ws = space, tab, CR, LF) as its number; hex, `Infinity`, separators and
/// anything else NaN (the reference's `NUMERIC`).
fn js_number_from_str(s: &str) -> f64 {
    let s = s.trim_matches([' ', '\t', '\n', '\r']);
    let b = s.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
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
    s.parse::<f64>().unwrap_or(f64::NAN)
}

fn opt_digits(v: Option<&Value>) -> Option<u32> {
    match v {
        None | Some(Value::Null) => None,
        Some(v) => fixed_digits(to_num(Some(v))),
    }
}

fn grouping(v: Option<&Value>) -> bool {
    !matches!(v, Some(Value::Bool(false)))
}

/// One format function over `formatter`; `now` = the clock
/// `formatRelativeTime` reads without a `now` argument. `None` = not a
/// format function, `Some(None)` = an undefined result.
pub fn call_format_function(name: &str, args: &Map<String, Value>, formatter: &dyn Formatter, now: f64) -> Option<Option<Value>> {
    let a = |k: &str| args.get(k);
    let text = |s: String| Some(Value::String(s));
    let number_or = |f: &dyn Fn(f64) -> String| {
        let n = to_num(a("value"));
        text(if n.is_finite() { f(n) } else { String::new() })
    };
    Some(match name {
        "formatNumber" => number_or(&|n| formatter.number(n, NumberOptions { decimals: opt_digits(a("decimals")), grouping: grouping(a("grouping")) })),
        "formatCurrency" => number_or(&|n| match a("currency").and_then(Value::as_str) {
            Some(code) if code.len() == 3 && code.bytes().all(|b| b.is_ascii_alphabetic()) => formatter.currency(n, &code.to_uppercase(), NumberOptions { decimals: opt_digits(a("decimals")), grouping: grouping(a("grouping")) }),
            _ => String::new(),
        }),
        "formatPercent" => number_or(&|n| formatter.percent(n, opt_digits(a("decimals")))),
        "formatDate" => {
            let options = DateOptions {
                format: a("format").and_then(Value::as_str).filter(|f| !f.is_empty()).map(str::to_string),
                style: a("style").and_then(Value::as_str).and_then(DateStyle::parse),
                time: a("time") == Some(&Value::Bool(true)),
            };
            text(format_date_value(formatter, a("value").unwrap_or(&Value::Null), &options))
        }
        "formatRelativeTime" => {
            let at = match a("now") {
                Some(Value::Number(n)) => n.as_f64().unwrap_or(now),
                Some(v) => parse_date_value(v).map(|d| d.ms).unwrap_or(now),
                None => now,
            };
            text(format_relative_value(formatter, a("value").unwrap_or(&Value::Null), at))
        }
        "pluralize" => {
            let arm = |k: &str| a(k).filter(|v| !v.is_null()).cloned();
            let n = to_num(a("value"));
            if !n.is_finite() {
                arm("other")
            } else if n == 0.0 && a("zero").is_some() {
                a("zero").cloned()
            } else {
                arm(formatter.plural(n).as_str()).or_else(|| arm("other"))
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn display_strings_follow_ecmascript() {
        for (v, s) in [(json!(412), "412"), (json!(1.5), "1.5"), (json!(0.30000000000000004), "0.30000000000000004"), (json!(1e-7), "1e-7"), (json!(1e21), "1e+21"), (json!(-0.0), "0"), (json!(true), "true"), (json!(null), ""), (json!([1]), "")] {
            assert_eq!(display_string(&v), s, "{v}");
        }
    }

    #[test]
    fn dates_round_trip_through_the_civil_calendar() {
        let f = utc_fields(946_684_799_000.0);
        assert_eq!((f.year, f.month, f.day, f.weekday, f.hour, f.minute, f.second), (1999, 12, 31, 5, 23, 59, 59));
        assert_eq!(parse_date_value(&json!("2026-10-14T00:30:00+02:00")).map(|d| utc_fields(d.ms).day), Some(13));
        assert!(parse_date_value(&json!("next tuesday")).is_none());
        let f = utc_fields(-1.0);
        assert_eq!((f.year, f.month, f.day, f.hour), (1969, 12, 31, 23));
    }

    #[test]
    fn a_host_formatter_replaces_the_fallback() {
        struct Loud;
        impl Formatter for Loud {
            fn locale(&self) -> String {
                "x".into()
            }
            fn number(&self, v: f64, _: NumberOptions) -> String {
                format!("#{v}")
            }
            fn currency(&self, _: f64, _: &str, _: NumberOptions) -> String {
                String::new()
            }
            fn percent(&self, _: f64, _: Option<u32>) -> String {
                String::new()
            }
            fn date(&self, _: DateValue, _: &DateOptions) -> String {
                String::new()
            }
            fn relative_time(&self, _: i64, _: RelativeUnit) -> String {
                String::new()
            }
            fn plural(&self, _: f64) -> PluralCategory {
                PluralCategory::Other
            }
        }
        let args = json!({"value": 3}).as_object().cloned().unwrap();
        assert_eq!(call_format_function("formatNumber", &args, &Loud, 0.0), Some(Some(json!("#3"))));
        let args = json!({"value": 1000}).as_object().cloned().unwrap();
        assert_eq!(call_format_function("formatRelativeTime", &args, &ENGLISH, 61_000.0), Some(Some(json!("1 minute ago"))));
    }

    /// A host formatter with other separators and digits (`map` = the
    /// group, the decimal and a digit transform over the English text).
    struct Seps(&'static str, &'static str, bool);
    impl Formatter for Seps {
        fn locale(&self) -> String {
            "x".into()
        }
        fn number(&self, v: f64, o: NumberOptions) -> String {
            let en = ENGLISH.number(v, o).replace(',', "\u{0}").replace('.', self.1).replace('\u{0}', self.0);
            if self.2 {
                en.chars().map(|c| c.to_digit(10).map(|d| char::from_u32(0x660 + d).unwrap()).unwrap_or(c)).collect()
            } else {
                en
            }
        }
        fn currency(&self, _: f64, _: &str, _: NumberOptions) -> String {
            String::new()
        }
        fn percent(&self, _: f64, _: Option<u32>) -> String {
            String::new()
        }
        fn date(&self, _: DateValue, _: &DateOptions) -> String {
            String::new()
        }
        fn relative_time(&self, _: i64, _: RelativeUnit) -> String {
            String::new()
        }
        fn plural(&self, _: f64) -> PluralCategory {
            PluralCategory::Other
        }
    }

    #[test]
    fn typed_numbers_parse_in_the_formatters_separators_and_digits() {
        assert_eq!(parse_number(&ENGLISH, "1,234.50"), Some(1234.5));
        assert_eq!(parse_number(&ENGLISH, " −2.5 "), Some(-2.5));
        assert_eq!(parse_number(&ENGLISH, ""), None);
        assert_eq!(parse_number(&ENGLISH, "-"), None);
        assert_eq!(parse_number(&ENGLISH, "abc"), None);
        let de = Seps(".", ",", false);
        assert_eq!(de.number(1234.5, NumberOptions { decimals: Some(2), grouping: true }), "1.234,50");
        assert_eq!(parse_number(&de, "2,50"), Some(2.5));
        assert_eq!(parse_number(&de, "1.234,50"), Some(1234.5));
        let fr = Seps("\u{202f}", ",", false);
        assert_eq!(parse_number(&fr, "1\u{202f}234,5"), Some(1234.5));
        let ar = Seps("٬", "٫", true);
        assert_eq!(parse_number(&ar, "١٬٢٣٤٫٥"), Some(1234.5));
        assert_eq!(parse_number(&ar, "\u{61c}-٧٫٠"), Some(-7.0));
    }

    #[test]
    fn numeric_strings_follow_the_reference_pattern() {
        for (text, want) in [("12", 12.0), (" -1.5e3\n", -1500.0), ("+.5", 0.5), ("1.", 1.0), ("2E-2", 0.02)] {
            assert_eq!(js_number_from_str(text), want, "{text:?}");
        }
        for text in ["", " ", "0x10", "Infinity", "1,000", "1e", ".", "-", "\u{a0}1", "1 2", "e5"] {
            assert!(js_number_from_str(text).is_nan(), "{text:?}");
        }
    }

    #[test]
    fn the_zoned_fallback_shifts_instants_never_days() {
        let berlin = ZonedEnglishFormatter::fixed(120);
        let at = parse_date_value(&json!("2026-10-14T22:30:00Z")).unwrap();
        assert_eq!(berlin.date(at, &DateOptions { format: Some("yyyy-MM-dd HH:mm".into()), ..Default::default() }), "2026-10-15 00:30");
        let day = parse_date_value(&json!("2026-10-14")).unwrap();
        assert_eq!(ZonedEnglishFormatter::fixed(840).date(day, &DateOptions { style: Some(DateStyle::Long), ..Default::default() }), "October 14, 2026");
        assert_eq!(ENGLISH.currency(1.0, "XYZ", NumberOptions::default()), "XYZ\u{a0}1.00");
    }
}
