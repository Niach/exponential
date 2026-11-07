//! The server's timestamp text (`2026-11-07T00:00:00.000Z`): what every row
//! of the store carries, so a local row reads like a synced one.

use jiff::{tz::TimeZone, Timestamp};

/// Now, as the server writes it (UTC, milliseconds, `Z`).
pub fn now() -> String {
    format_ms(Timestamp::now().as_millisecond())
}

/// A millisecond instant as the server writes it.
pub fn format_ms(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let zoned = Timestamp::from_second(secs).unwrap_or(Timestamp::UNIX_EPOCH).to_zoned(TimeZone::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        zoned.year(),
        zoned.month(),
        zoned.day(),
        zoned.hour(),
        zoned.minute(),
        zoned.second(),
        millis
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_server() {
        assert_eq!(format_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_ms(1_794_009_600_123), "2026-11-07T00:00:00.123Z");
        assert!(now().ends_with('Z'));
        assert_eq!(now().len(), 24);
    }
}
