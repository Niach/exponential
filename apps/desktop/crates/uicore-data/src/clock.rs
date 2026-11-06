//! The clock the documents bind: `/time` (`HH:MM`, local), `/date`
//! (`yyyy-mm-dd`, local) and `/dateText` (the calendar applet's words for
//! the same day, #EDE-43). `EDE_SHELL_CLOCK` pins the time (the goldens,
//! #EDE-11) and `EDE_SHELL_DATE` the date.

use serde_json::{json, Value};

/// `HH:MM` local time; `EDE_SHELL_CLOCK` pins it.
pub fn time() -> String {
    if let Some(fixed) = std::env::var_os("EDE_SHELL_CLOCK").filter(|v| !v.is_empty()) {
        return fixed.to_string_lossy().into_owned();
    }
    let tm = local();
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// `yyyy-mm-dd` local date; `EDE_SHELL_DATE` pins it.
pub fn date() -> String {
    if let Some(fixed) = std::env::var_os("EDE_SHELL_DATE").filter(|v| !v.is_empty()) {
        return fixed.to_string_lossy().into_owned();
    }
    let tm = local();
    format!("{:04}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
}

/// `yyyy-mm-dd HH:MM` local for a unix time (a history entry).
pub fn stamp(epoch: i64) -> String {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t: libc::time_t = epoch as libc::time_t;
    // SAFETY: localtime_r writes into the tm we own.
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

/// `/dateText`: `{weekday, date, long}` for [`date`] (`Friday`,
/// `6 November 2026`, `Friday, 6 November 2026`), in English; a pinned
/// date gives its own words.
pub fn date_text() -> Value {
    date_words(&date())
}

/// The words for a `yyyy-mm-dd` (an unparsable one: empty strings).
pub fn date_words(ymd: &str) -> Value {
    let mut parts = ymd.split('-').map(|p| p.parse::<i64>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next()) else {
        return json!({"weekday": "", "date": "", "long": ""});
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return json!({"weekday": "", "date": "", "long": ""});
    }
    const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    // Sakamoto's day of the week, 0 = Sunday.
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    let dow = (yy + yy / 4 - yy / 100 + yy / 400 + T[(m - 1) as usize] + d).rem_euclid(7);
    let weekday = WEEKDAYS[dow as usize];
    let date = format!("{d} {} {y}", MONTHS[(m - 1) as usize]);
    json!({"weekday": weekday, "date": date, "long": format!("{weekday}, {date}")})
}

fn local() -> libc::tm {
    // SAFETY: time() with a null pointer only returns; localtime_r writes
    // into the tm we own.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        libc::localtime_r(&now, &mut tm);
    }
    tm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_of_a_date() {
        assert_eq!(date_words("2026-11-06"), json!({"weekday": "Friday", "date": "6 November 2026", "long": "Friday, 6 November 2026"}));
        assert_eq!(date_words("2024-02-29")["weekday"], "Thursday");
        assert_eq!(date_words("2000-01-01")["long"], "Saturday, 1 January 2000");
        assert_eq!(date_words("nope")["weekday"], "");
    }
}
