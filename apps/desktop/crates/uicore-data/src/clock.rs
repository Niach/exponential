//! The clock the documents bind: `/time` (`HH:MM`, local) and `/date`
//! (`yyyy-mm-dd`, local). `EDE_SHELL_CLOCK` pins the time (the goldens,
//! #EDE-11) and `EDE_SHELL_DATE` the date.

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
