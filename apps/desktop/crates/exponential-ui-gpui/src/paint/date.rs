//! Calendar arithmetic for the DatePicker popup (proleptic Gregorian, no
//! time zones: values are `YYYY-MM-DD` strings like the catalog's).

/// `"2026-10-14"` (anything after the day ignored) → (year, month, day).
pub fn parse_iso(s: &str) -> Option<(i32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some((y, m, d))
}

pub fn iso(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// Monday = 0 … Sunday = 6.
pub fn weekday_monday0(y: i32, m: u32, d: u32) -> u32 {
    // 1970-01-01 was a Thursday (= 3).
    ((days_from_civil(y, m, d) + 3).rem_euclid(7)) as u32
}

/// `(year, month)` shifted by `delta` months.
pub fn add_months(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let total = y * 12 + (m as i32 - 1) + delta;
    (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32)
}

/// The 42 cells (6 weeks, Monday first) of a month view: (y, m, d, in_month).
pub fn month_grid(y: i32, m: u32) -> Vec<(i32, u32, u32, bool)> {
    let start = days_from_civil(y, m, 1) - weekday_monday0(y, m, 1) as i64;
    (0..42)
        .map(|i| {
            let (cy, cm, cd) = civil_from_days(start + i);
            (cy, cm, cd, cy == y && cm == m)
        })
        .collect()
}

pub const MONTH_NAMES: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const WEEKDAYS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// Today in UTC (the calendar's initial month without a value).
pub fn today() -> (i32, u32, u32) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    civil_from_days(secs.div_euclid(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        assert_eq!(parse_iso("2026-10-14"), Some((2026, 10, 14)));
        assert_eq!(parse_iso("2026-02-29"), None);
        assert_eq!(parse_iso("2024-02-29"), Some((2024, 2, 29)));
        assert_eq!(parse_iso("2026-10-14T10:00"), Some((2026, 10, 14)));
        assert_eq!(iso(2026, 1, 5), "2026-01-05");
        for z in [-1000, 0, 365, 20_000, 20_740] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(weekday_monday0(2026, 10, 8), 3); // a Thursday
        assert_eq!(add_months(2026, 1, -1), (2025, 12));
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
        let grid = month_grid(2026, 10);
        assert_eq!(grid.len(), 42);
        assert_eq!(grid[0], (2026, 9, 28, false));
        assert_eq!(grid[3], (2026, 10, 1, true));
    }
}
