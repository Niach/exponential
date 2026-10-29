//! A row's timestamp as UTC milliseconds: RFC 3339 or the Postgres spelling
//! the shapes deliver (`2026-10-25 10:00:05.123+00`); zone-less means UTC.
//! The same ladder as `crate::instant::parse_instant` (eTalk is a leaf crate,
//! so it keeps its own copy).

pub fn parse_instant(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp_millis());
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f%#z", "%Y-%m-%dT%H:%M:%S%.f%#z", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(parsed) = chrono::DateTime::parse_from_str(text, format) {
            return Some(parsed.timestamp_millis());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(text, format) {
            return Some(parsed.and_utc().timestamp_millis());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_instant;

    #[test]
    fn reads_both_spellings() {
        assert_eq!(parse_instant("2026-10-25T10:00:05.123Z"), Some(1_792_922_405_123));
        assert_eq!(parse_instant("2026-10-25 10:00:05.123+00"), Some(1_792_922_405_123));
        assert_eq!(parse_instant("2026-10-25 10:00:05.123"), Some(1_792_922_405_123));
        assert_eq!(parse_instant("nope"), None);
    }
}
