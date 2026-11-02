//! VAPP-190: calendar-day maths, the Rust twin of
//! `packages/exponential-ui/src/dates.ts`, shared by the records query
//! (time buckets, `host::query`) and the bind-time expressions (Q2:
//! dateAdd, startOf, isoWeek…). Pure and proleptic Gregorian over
//! `YYYY-MM-DD` texts, no clock and no locale; `fixtures/records-query.json`
//! `bucketCases` lock it. A date is a calendar day (spec §4.3); a
//! timestamp's day is the first ten characters of its ISO text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CalendarDay {
    pub y: i64,
    pub m: i64,
    pub d: i64,
}

/// The time buckets a query groups dates by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateBucket {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

pub const DATE_BUCKETS: [&str; 5] = ["day", "week", "month", "quarter", "year"];

impl DateBucket {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "quarter" => Some(Self::Quarter),
            "year" => Some(Self::Year),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Year => "year",
        }
    }
}

pub fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap_year(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// 0 = Sunday (Sakamoto's method).
pub fn weekday_of(y: i64, m: i64, d: i64) -> usize {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[(m.clamp(1, 12) - 1) as usize] + d).rem_euclid(7) as usize
}

/// 1-based day of the year.
pub fn day_of_year(y: i64, m: i64, d: i64) -> i64 {
    (1..m).map(|i| days_in_month(y, i)).sum::<i64>() + d
}

/// ISO 8601 weeks in a year: 53 when it starts on a Thursday (or a Wednesday in a leap year).
pub fn iso_weeks_in_year(y: i64) -> i64 {
    let jan1 = weekday_of(y, 1, 1);
    if jan1 == 4 || (jan1 == 3 && is_leap_year(y)) {
        53
    } else {
        52
    }
}

/// The ISO week (Monday first, week 1 holds the first Thursday): `(year, week)`.
pub fn iso_week(y: i64, m: i64, d: i64) -> (i64, i64) {
    let wd = match weekday_of(y, m, d) {
        0 => 7,
        n => n as i64,
    };
    let week = (day_of_year(y, m, d) - wd + 10).div_euclid(7);
    if week < 1 {
        return (y - 1, iso_weeks_in_year(y - 1));
    }
    if week > iso_weeks_in_year(y) {
        return (y + 1, 1);
    }
    (y, week)
}

fn digits(b: &[u8], from: usize, n: usize) -> bool {
    b.len() >= from + n && b[from..from + n].iter().all(u8::is_ascii_digit)
}

fn int(b: &[u8], from: usize, n: usize) -> i64 {
    b[from..from + n].iter().fold(0, |acc, c| acc * 10 + i64::from(c - b'0'))
}

/// The calendar day at the start of `text` (`YYYY-MM-DD`, then the end, `T`
/// or a space); `None` when it is not a real day.
pub fn parse_day(text: &str) -> Option<CalendarDay> {
    let b = text.as_bytes();
    if !(digits(b, 0, 4) && b.get(4) == Some(&b'-') && digits(b, 5, 2) && b.get(7) == Some(&b'-') && digits(b, 8, 2)) {
        return None;
    }
    if b.len() > 10 && b[10] != b'T' && b[10] != b' ' {
        return None;
    }
    let (y, m, d) = (int(b, 0, 4), int(b, 5, 2), int(b, 8, 2));
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(CalendarDay { y, m, d })
}

/// The year and month at the start of `text` (`YYYY-MM`, then the end or `-`).
pub fn parse_month(text: &str) -> Option<(i64, i64)> {
    let b = text.as_bytes();
    if !(digits(b, 0, 4) && b.get(4) == Some(&b'-') && digits(b, 5, 2)) || (b.len() > 7 && b[7] != b'-') {
        return None;
    }
    let m = int(b, 5, 2);
    (1..=12).contains(&m).then(|| (int(b, 0, 4), m))
}

/// The year at the start of `text` (`YYYY`, then the end or `-`).
pub fn parse_year(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if !digits(b, 0, 4) || (b.len() > 4 && b[4] != b'-') {
        return None;
    }
    Some(int(b, 0, 4))
}

/// `YYYY-MM-DD`.
pub fn format_day(day: CalendarDay) -> String {
    format!("{:04}-{:02}-{:02}", day.y, day.m, day.d)
}

/// A date's bucket key: day `2026-10-05`, week `2026-W41` (ISO), month
/// `2026-10`, quarter `2026-Q4`, year `2026`; `None` when the text holds no
/// date that precise.
pub fn date_bucket(text: &str, bucket: DateBucket) -> Option<String> {
    match bucket {
        DateBucket::Day => parse_day(text).map(format_day),
        DateBucket::Week => parse_day(text).map(|day| {
            let (year, week) = iso_week(day.y, day.m, day.d);
            format!("{year:04}-W{week:02}")
        }),
        DateBucket::Year => parse_year(text).map(|y| format!("{y:04}")),
        DateBucket::Month => parse_month(text).map(|(y, m)| format!("{y:04}-{m:02}")),
        DateBucket::Quarter => parse_month(text).map(|(y, m)| format!("{y:04}-Q{}", (m - 1) / 3 + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_weeks_cross_years() {
        assert_eq!(date_bucket("2027-01-01", DateBucket::Week).as_deref(), Some("2026-W53"));
        assert_eq!(date_bucket("2024-12-30", DateBucket::Week).as_deref(), Some("2025-W01"));
        assert_eq!(date_bucket("2026-02-30", DateBucket::Day), None);
    }
}
