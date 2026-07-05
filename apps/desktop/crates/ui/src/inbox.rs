//! Inbox helpers (masterplan-v3 §4.2). The full-page Inbox screen is gone —
//! the rail's Inbox tool window (a mini list in `sidebar.rs`) replaced it —
//! but the time formatting it shares with other surfaces lives on here.

/// Web `relativeTime`: "just now" / "Nm" / "Nh" / "Nd" (rounded like JS).
pub(crate) fn relative_time(created_at: &str) -> String {
    let Some(then) = parse_epoch_seconds(created_at) else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    relative_time_between(now, then)
}

fn relative_time_between(now: i64, then: i64) -> String {
    let diff = (now - then).max(0);
    let mins = (diff as f64 / 60.).round() as i64;
    if mins < 1 {
        return "just now".to_string();
    }
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = (mins as f64 / 60.).round() as i64;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", (hours as f64 / 24.).round() as i64)
}

/// Tolerant ISO-8601 → epoch seconds. Electric forwards Postgres `timestamptz`
/// text (`2026-07-03 10:11:12.345+00` — space separator, short offset), tRPC
/// echoes RFC 3339; accept both.
fn parse_epoch_seconds(value: &str) -> Option<i64> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp());
    }
    for format in [
        "%Y-%m-%d %H:%M:%S%.f%#z",
        "%Y-%m-%dT%H:%M:%S%.f%#z",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(parsed) = chrono::DateTime::parse_from_str(value, format) {
            return Some(parsed.timestamp());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.and_utc().timestamp());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_rounds_like_the_web() {
        let now = 1_800_000_000_i64;
        assert_eq!(relative_time_between(now, now - 20), "just now");
        assert_eq!(relative_time_between(now, now - 120), "2m");
        assert_eq!(relative_time_between(now, now - 3 * 3600), "3h");
        assert_eq!(relative_time_between(now, now - 26 * 3600), "1d");
        assert_eq!(relative_time_between(now, now - 50 * 3600), "2d");
        // Future timestamps clamp to "just now" instead of going negative.
        assert_eq!(relative_time_between(now, now + 3600), "just now");
    }

    #[test]
    fn epoch_parser_accepts_postgres_and_rfc3339_forms() {
        // RFC 3339 (tRPC / ISO).
        assert!(parse_epoch_seconds("2026-07-03T10:11:12.345+00:00").is_some());
        assert!(parse_epoch_seconds("2026-07-03T10:11:12Z").is_some());
        // Postgres text form Electric forwards (space + short offset).
        assert!(parse_epoch_seconds("2026-07-03 10:11:12.345+00").is_some());
        assert!(parse_epoch_seconds("2026-07-03 10:11:12+00").is_some());
        // Naive fallback.
        assert!(parse_epoch_seconds("2026-07-03 10:11:12").is_some());
        assert!(parse_epoch_seconds("garbage").is_none());

        // The two zoned forms agree on the instant.
        assert_eq!(
            parse_epoch_seconds("2026-07-03T10:11:12+00:00"),
            parse_epoch_seconds("2026-07-03 10:11:12+00"),
        );
    }
}
