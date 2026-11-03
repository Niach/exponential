//! eApps U3 (VAPP-232, spec §17 U3): the CALENDAR half of the records
//! query, the Rust twin of `packages/exponential-ui/src/host/calendar.ts`
//! (fixtures/records-query.json locks both): the month view's weeks with
//! their lanes, the week view and the timeline. Pure day maths over day
//! numbers (`crate::dates`). `host::query` filters and sorts the rows, then
//! hands each placed row here as a [`CalendarSpan`].

use serde_json::{json, Map, Value};

use super::utf16_cmp;
use crate::dates::{day_from_number, day_number, format_day, parse_day, CalendarDay};

pub const CALENDAR_LANES_DEFAULT: u32 = 3;
pub const CALENDAR_LANES_MAX: u32 = 8;
pub const TIMELINE_COLUMNS_DEFAULT: u32 = 12;
pub const TIMELINE_COLUMNS_MAX: u32 = 52;
pub const TIMELINE_SCALES: [&str; 3] = ["week", "month", "quarter"];
pub const CALENDAR_TONES: [&str; 8] = ["chart1", "chart2", "chart3", "chart4", "chart5", "chart6", "chart7", "chart8"];
/// Monday first (the calendar's week; `RECORDS_WEEKDAYS` stays Sunday first).
pub const CALENDAR_WEEKDAYS_LONG: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// One placed row: its first and last day (inclusive), its tone, its value
/// row, its place in the query's order and (timeline) its row key.
#[derive(Debug, Clone)]
pub struct CalendarSpan {
    pub id: String,
    pub start: i64,
    pub end: i64,
    pub tone: String,
    pub value: Value,
    pub order: usize,
    pub row_key: Option<String>,
}

/// A real day's number; `None` = not a day.
pub fn day_of(text: &str) -> Option<i64> {
    parse_day(text).map(|d| day_number(d.y, d.m, d.d))
}

/// `start`..`end` as a row's span: `end` missing, not a day or before the
/// start = the start day only; `None` = the start is no day.
pub fn span_of(start_text: &str, end_text: Option<&str>) -> Option<(i64, i64)> {
    let start = day_of(start_text)?;
    let end = end_text.and_then(day_of);
    Some((start, end.filter(|e| *e >= start).unwrap_or(start)))
}

pub fn day_text(n: i64) -> String {
    format_day(day_from_number(n))
}

/// 0 = Monday (1970-01-01, day 0, was a Thursday).
fn weekday_index(n: i64) -> usize {
    (n + 3).rem_euclid(7) as usize
}

fn monday_of(n: i64) -> i64 {
    n - weekday_index(n) as i64
}

fn is_tone_digit(v: &str) -> bool {
    v.len() == 1 && matches!(v.as_bytes()[0], b'1'..=b'8')
}

/// The tones of `values` (each placed row's tone field as text): an integer
/// 1–8 is that chart tone; any other non-empty text the tone of its rank
/// among the distinct such texts (code units); empty = chart1.
pub fn tones_of(values: &[String]) -> Vec<String> {
    let mut named: Vec<&str> = values.iter().map(String::as_str).filter(|v| !v.is_empty() && !is_tone_digit(v)).collect();
    named.sort_by(|a, b| utf16_cmp(a, b));
    named.dedup();
    values
        .iter()
        .map(|v| {
            if is_tone_digit(v) {
                format!("chart{v}")
            } else if v.is_empty() {
                "chart1".to_string()
            } else {
                let rank = named.iter().position(|n| *n == v.as_str()).unwrap_or(0);
                CALENDAR_TONES[rank % 8].to_string()
            }
        })
        .collect()
}

/// Spans clipped to `[from, to]` in lane order: by first day, then the
/// longer first, then the query's order.
fn lane_order(spans: &[CalendarSpan], from: i64, to: i64) -> Vec<&CalendarSpan> {
    let mut out: Vec<&CalendarSpan> = spans.iter().filter(|s| s.end >= from && s.start <= to).collect();
    out.sort_by(|a, b| {
        let (af, bf) = (a.start.max(from), b.start.max(from));
        let (al, bl) = (a.end.min(to) - af, b.end.min(to) - bf);
        af.cmp(&bf).then(bl.cmp(&al)).then(a.order.cmp(&b.order))
    });
    out
}

/// The lowest lane whose cells `first..=last` are all free (`max` lanes;
/// `None` = none), marking them taken.
fn take_lane(lanes: &mut Vec<Vec<bool>>, first: usize, last: usize, max: usize) -> Option<usize> {
    for lane in 0..max {
        if lanes.len() <= lane {
            lanes.push(Vec::new());
        }
        let row = &mut lanes[lane];
        if row.len() <= last {
            row.resize(last + 1, false);
        }
        if row[first..=last].iter().any(|c| *c) {
            continue;
        }
        row[first..=last].iter_mut().for_each(|c| *c = true);
        return Some(lane);
    }
    None
}

pub struct WeekOptions {
    pub lanes: u32,
    pub today: Option<i64>,
    pub selected: Option<i64>,
    pub month: Option<i64>,
}

/// One Monday-first week (`calendarWeek` in calendar.ts): {key, days, segments}.
pub fn calendar_week(monday: i64, spans: &[CalendarSpan], o: &WeekOptions) -> Map<String, Value> {
    let sunday = monday + 6;
    let placed = lane_order(spans, monday, sunday);
    let mut lanes: Vec<Vec<bool>> = Vec::new();
    let mut hidden = [0u64; 7];
    let mut segments: Vec<Value> = Vec::new();
    for s in &placed {
        let first = (s.start.max(monday) - monday) as usize;
        let last = (s.end.min(sunday) - monday) as usize;
        match take_lane(&mut lanes, first, last, o.lanes as usize) {
            None => hidden[first..=last].iter_mut().for_each(|h| *h += 1),
            Some(lane) => segments.push(json!({
                "id": s.id, "col": first + 1, "span": last - first + 1, "lane": lane,
                "contL": s.start < monday, "contR": s.end > sunday, "tone": s.tone, "more": 0, "row": s.value,
            })),
        }
    }
    for (c, n) in hidden.iter().enumerate() {
        if *n > 0 {
            segments.push(json!({
                "id": format!("more:{}", day_text(monday + c as i64)), "col": c + 1, "span": 1, "lane": o.lanes,
                "contL": false, "contR": false, "tone": null, "more": n, "row": null,
            }));
        }
    }
    let days: Vec<Value> = (0..7)
        .map(|c| {
            let n = monday + c as i64;
            let d = day_from_number(n);
            let covering: Vec<&&CalendarSpan> = placed.iter().filter(|s| s.start <= n && s.end >= n).collect();
            let dots: Vec<&str> = covering.iter().take(3).map(|s| s.tone.as_str()).collect();
            json!({
                "date": format_day(d), "day": d.d, "weekday": WEEKDAYS[c], "initial": &WEEKDAYS[c][..1],
                "inMonth": o.month.map_or(true, |m| d.m == m), "weekend": c >= 5, "today": Some(n) == o.today,
                "selected": Some(n) == o.selected, "total": covering.len(), "dots": dots,
            })
        })
        .collect();
    let mut out = Map::new();
    out.insert("key".into(), json!(day_text(monday)));
    out.insert("days".into(), Value::Array(days));
    out.insert("segments".into(), Value::Array(segments));
    out
}

/// `2 – 8 November 2026`, `30 November – 6 December 2026`, `29 December 2025 – 4 January 2026`.
pub fn range_label(from: i64, to: i64) -> String {
    let a = day_from_number(from);
    let b = day_from_number(to);
    let tail = format!("{} {} {}", b.d, MONTHS[(b.m - 1) as usize], b.y);
    if a.y != b.y {
        format!("{} {} {} – {tail}", a.d, MONTHS[(a.m - 1) as usize], a.y)
    } else if a.m != b.m {
        format!("{} {} – {tail}", a.d, MONTHS[(a.m - 1) as usize])
    } else {
        format!("{} – {tail}", a.d)
    }
}

/// `Tuesday, 3 November`.
pub fn day_label(n: i64) -> String {
    let d = day_from_number(n);
    format!("{}, {} {}", CALENDAR_WEEKDAYS_LONG[weekday_index(n)], d.d, MONTHS[(d.m - 1) as usize])
}

/// The month view's calendar half: {weeks, day, agenda} (`calendarMonth`).
pub fn calendar_month(y: i64, m: i64, last_day: i64, spans: &[CalendarSpan], lanes: u32, today: i64, day: Option<&str>) -> Map<String, Value> {
    let first = day_number(y, m, 1);
    let last = day_number(y, m, last_day);
    let asked = day.and_then(day_of);
    let selected = match asked {
        Some(a) if a >= first && a <= last => a,
        _ if today >= first && today <= last => today,
        _ => first,
    };
    let mut weeks = Vec::new();
    let mut monday = monday_of(first);
    while monday <= last {
        weeks.push(Value::Object(calendar_week(monday, spans, &WeekOptions { lanes, today: Some(today), selected: Some(selected), month: Some(m) })));
        monday += 7;
    }
    let items: Vec<Value> = lane_order(spans, selected, selected)
        .iter()
        .map(|s| json!({"id": s.id, "tone": s.tone, "start": day_text(s.start), "end": day_text(s.end), "days": s.end - s.start + 1, "row": s.value}))
        .collect();
    let mut out = Map::new();
    out.insert("weeks".into(), Value::Array(weeks));
    out.insert("day".into(), json!(day_text(selected)));
    out.insert("agenda".into(), json!({"date": day_text(selected), "label": day_label(selected), "count": items.len(), "items": items}));
    out
}

/// The week view: the Monday-first week holding `key`.
pub fn calendar_week_view(key: i64, spans: &[CalendarSpan], lanes: u32, today: i64) -> Value {
    let monday = monday_of(key);
    let week = calendar_week(monday, spans, &WeekOptions { lanes, today: Some(today), selected: None, month: None });
    json!({
        "key": day_text(monday), "label": range_label(monday, monday + 6), "prev": day_text(monday - 7), "next": day_text(monday + 7),
        "today": day_text(today), "days": week["days"], "segments": week["segments"],
    })
}

/// The first day of the scale period holding day `n`.
pub fn period_start(n: i64, scale: &str) -> i64 {
    if scale == "week" {
        return monday_of(n);
    }
    let d = day_from_number(n);
    day_number(d.y, if scale == "month" { d.m } else { (d.m - 1) / 3 * 3 + 1 }, 1)
}

/// `start` (a period's first day) moved by `k` periods.
pub fn shift_period(start: i64, scale: &str, k: i64) -> i64 {
    if scale == "week" {
        return start + 7 * k;
    }
    let d = day_from_number(start);
    let index = d.y * 12 + (d.m - 1) + k * if scale == "month" { 1 } else { 3 };
    day_number(index.div_euclid(12), index.rem_euclid(12) + 1, 1)
}

fn column_label(d: CalendarDay, scale: &str) -> String {
    let short = &MONTHS[(d.m - 1) as usize][..3];
    match scale {
        "week" => format!("{} {short}", d.d),
        "month" => short.to_string(),
        _ => format!("Q{}", (d.m - 1) / 3 + 1),
    }
}

/// The timeline (`calendarTimeline` in calendar.ts).
pub fn calendar_timeline(start: i64, spans: &[CalendarSpan], scale: &str, columns: u32, today: i64) -> Value {
    let cols = columns as usize;
    let p0 = period_start(start, scale);
    let starts: Vec<i64> = (0..=cols as i64).map(|i| shift_period(p0, scale, i)).collect();
    let axis_end = starts[cols] - 1;
    let col_of = |n: i64| -> usize {
        let mut c = 0;
        while c + 1 < cols && starts[c + 1] <= n {
            c += 1;
        }
        c
    };
    let axis_year = day_from_number(p0).y;
    let columns_json: Vec<Value> = (0..cols)
        .map(|i| json!({"key": day_text(starts[i]), "label": column_label(day_from_number(starts[i]), scale), "end": day_text(starts[i + 1] - 1), "today": today >= starts[i] && today < starts[i + 1]}))
        .collect();
    let mut bands: Vec<(String, usize, usize)> = Vec::new();
    for (i, s) in starts.iter().take(cols).enumerate() {
        let d = day_from_number(*s);
        let label = if scale == "week" {
            if d.y == axis_year { MONTHS[(d.m - 1) as usize].to_string() } else { format!("{} {}", MONTHS[(d.m - 1) as usize], d.y) }
        } else {
            d.y.to_string()
        };
        match bands.last_mut() {
            Some(last) if last.0 == label => last.2 += 1,
            _ => bands.push((label, i + 1, 1)),
        }
    }
    let mut ordered: Vec<&CalendarSpan> = spans.iter().collect();
    ordered.sort_by_key(|s| s.order);
    let mut groups: Vec<(String, Vec<&CalendarSpan>)> = Vec::new();
    for s in ordered {
        if s.end < p0 || s.start > axis_end {
            continue;
        }
        let key = s.row_key.clone().unwrap_or_else(|| s.id.clone());
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(s),
            None => groups.push((key, vec![s])),
        }
    }
    let rows: Vec<Value> = groups
        .into_iter()
        .enumerate()
        .map(|(index, (key, list))| {
            let mut lanes: Vec<Vec<bool>> = Vec::new();
            let mut cells: Vec<(&CalendarSpan, usize, usize)> = list.iter().map(|s| (*s, col_of(s.start.max(p0)), col_of(s.end.min(axis_end)))).collect();
            cells.sort_by(|a, b| a.1.cmp(&b.1).then((b.2 - b.1).cmp(&(a.2 - a.1))).then(a.0.order.cmp(&b.0.order)));
            let items: Vec<Value> = cells
                .iter()
                .map(|(s, first, last)| {
                    let lane = take_lane(&mut lanes, *first, *last, usize::MAX).unwrap_or(0);
                    json!({
                        "id": s.id, "rowIndex": index, "col": first + 1, "span": last - first + 1, "lane": lane,
                        "contL": s.start < p0, "contR": s.end > axis_end, "tone": s.tone, "row": s.value,
                    })
                })
                .collect();
            json!({"key": key, "index": index, "lanes": lanes.len().max(1), "count": items.len(), "items": items})
        })
        .collect();
    let now = if today >= p0 && today <= axis_end {
        let c = col_of(today);
        let frac = (today - starts[c]) as f64 / (starts[c + 1] - starts[c]) as f64;
        let percent = (((c as f64 + frac) / f64::from(columns)) * 10000.0).round() / 100.0;
        json!({"col": c + 1, "percent": crate::json::number(percent)})
    } else {
        Value::Null
    };
    json!({
        "scale": scale, "key": day_text(p0), "label": range_label(p0, axis_end),
        "prev": day_text(shift_period(p0, scale, -i64::from(columns))), "next": day_text(starts[cols]), "today": day_text(today),
        "columns": columns_json,
        "bands": bands.into_iter().map(|(label, col, span)| json!({"label": label, "col": col, "span": span})).collect::<Vec<_>>(),
        "rows": rows, "now": now,
    })
}
