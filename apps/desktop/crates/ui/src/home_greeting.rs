//! EXP-1253 (EXP-1259 D1) — the Agent home's greeting ("Evening, Danny"),
//! the desktop mirror of `packages/domain-contract/src/home-greeting.ts`.
//! Pure and byte-locked by `fixtures/composer-home.json`: the test below
//! replays its `cases` and pins the copy + geometry consts to it. The
//! fixture's `$comment` holds the rules.

use chrono::{NaiveDate, Timelike as _};

/// The composer's placeholder with no subject picked.
pub(crate) const HOME_PLACEHOLDER: &str = "What do you want to build?";

/// The fixture's `testIds`: the IDE has no DOM, so they name the home's
/// element ids (a capture or a test finds the same pieces as on the web).
pub(crate) const TEST_ID_HOME: &str = "agent-home";
pub(crate) const TEST_ID_GREETING: &str = "agent-home-greeting";
pub(crate) const TEST_ID_MARK: &str = "agent-home-mark";

/// The headline row's geometry (px): the disc mark, the gap to the
/// greeting, the greeting's size + weight, then the gap to the card and
/// (while the team has no board) from the options line to Getting started.
pub(crate) const MARK_PX: f32 = 32.;
pub(crate) const GAP_PX: f32 = 12.;
pub(crate) const HEADLINE_PX: f32 = 30.;
pub(crate) const HEADLINE_WEIGHT: f32 = 600.;
pub(crate) const HEADLINE_TO_CARD_PX: f32 = 24.;
pub(crate) const COMPOSER_TO_GETTING_STARTED_PX: f32 = 32.;

/// The greeting day starts at 05:00: one night keeps one variant.
const DAY_STARTS_AT_HOUR: i64 = 5;

struct Bucket {
    from: u32,
    to: u32,
    variants: &'static [&'static str],
}

const BUCKETS: [Bucket; 4] = [
    Bucket {
        from: 5,
        to: 11,
        variants: &[
            "Good morning, {name}",
            "Coffee and code, {name}?",
            "Morning, {name}",
            "Back at it, {name}",
        ],
    },
    Bucket {
        from: 12,
        to: 17,
        variants: &[
            "Good afternoon, {name}",
            "What's next, {name}?",
            "Afternoon, {name}",
            "Back at it, {name}",
        ],
    },
    Bucket {
        from: 18,
        to: 22,
        variants: &[
            "Good evening, {name}",
            "How was your day, {name}?",
            "Evening, {name}",
            "Back at it, {name}",
        ],
    },
    Bucket {
        from: 23,
        to: 4,
        variants: &[
            "Hello, night owl",
            "Burning the midnight oil, {name}?",
            "Night owl, {name}?",
            "Still up, {name}?",
        ],
    },
];

fn bucket_index(hour: u32) -> usize {
    BUCKETS
        .iter()
        .position(|bucket| {
            if bucket.from <= bucket.to {
                hour >= bucket.from && hour <= bucket.to
            } else {
                hour >= bucket.from || hour <= bucket.to
            }
        })
        .unwrap_or(0)
}

/// Whole days from 1970-01-01 to the greeting day of `date` + `hour` (the
/// local wall clock less [`DAY_STARTS_AT_HOUR`]).
fn day_index(date: NaiveDate, hour: u32) -> i64 {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("the epoch is a date");
    let days = date.signed_duration_since(epoch).num_days();
    (days * 24 + i64::from(hour) - DAY_STARTS_AT_HOUR).div_euclid(24)
}

/// What a greeting depends on besides the name: its greeting day and its
/// bucket. A mounted home re-reads its greeting only once this moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GreetingKey {
    day: i64,
    bucket: usize,
}

/// The [`GreetingKey`] of this device's local time now.
pub(crate) fn greeting_key_now() -> GreetingKey {
    let now = chrono::Local::now();
    GreetingKey {
        day: day_index(now.date_naive(), now.hour()),
        bucket: bucket_index(now.hour()),
    }
}

/// The first whitespace-separated token of a user row's name, or "".
pub(crate) fn greeting_first_name(name: Option<&str>) -> &str {
    name.unwrap_or("").split_whitespace().next().unwrap_or("")
}

/// The greeting for a local wall-clock date + hour and a user row's name.
pub(crate) fn home_greeting(date: NaiveDate, hour: u32, name: Option<&str>) -> String {
    let bucket = &BUCKETS[bucket_index(hour)];
    let count = bucket.variants.len() as i64;
    let variant = bucket.variants[day_index(date, hour).rem_euclid(count) as usize];
    let first = greeting_first_name(name);
    if first.is_empty() {
        variant.replace(", {name}", "")
    } else {
        variant.replace("{name}", first)
    }
}

/// [`home_greeting`] at this device's local time now. The home computes it
/// once per mount, so it never flips while you type.
pub(crate) fn home_greeting_now(name: Option<&str>) -> String {
    let now = chrono::Local::now();
    home_greeting(now.date_naive(), now.hour(), name)
}

/// `YYYY-MM-DDTHH:mm` → (date, hour), the fixture's `local` form.
#[cfg(test)]
fn parse_local(local: &str) -> Option<(NaiveDate, u32)> {
    use chrono::Datelike as _;
    let at = chrono::NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M").ok()?;
    let date = NaiveDate::from_ymd_opt(at.year(), at.month(), at.day())?;
    Some((date, at.hour()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/composer-home.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("composer-home.json parses")
    }

    #[test]
    fn every_case_greets_as_the_fixture_says() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for case in cases {
            let (date, hour) = parse_local(case["local"].as_str().unwrap()).expect("local");
            let user = case["user"].as_str();
            assert_eq!(
                home_greeting(date, hour, user),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn the_buckets_are_the_fixture_buckets() {
        let fixture = fixture();
        let buckets = fixture["buckets"].as_array().expect("buckets");
        assert_eq!(buckets.len(), BUCKETS.len());
        for (want, have) in buckets.iter().zip(BUCKETS.iter()) {
            assert_eq!(want["from"].as_u64().unwrap() as u32, have.from);
            assert_eq!(want["to"].as_u64().unwrap() as u32, have.to);
            let variants: Vec<&str> = want["variants"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(variants, have.variants);
        }
        assert_eq!(fixture["dayStartsAtHour"].as_i64().unwrap(), DAY_STARTS_AT_HOUR);
    }

    #[test]
    fn copy_and_geometry_are_the_fixture() {
        let fixture = fixture();
        assert_eq!(fixture["placeholder"].as_str().unwrap(), HOME_PLACEHOLDER);
        let g = &fixture["geometry"];
        let px = |key: &str| g[key].as_f64().unwrap() as f32;
        assert_eq!(px("mark"), MARK_PX);
        assert_eq!(px("gap"), GAP_PX);
        assert_eq!(px("headline"), HEADLINE_PX);
        assert_eq!(px("headlineWeight"), HEADLINE_WEIGHT);
        assert_eq!(px("headlineToCard"), HEADLINE_TO_CARD_PX);
        assert_eq!(px("composerToGettingStarted"), COMPOSER_TO_GETTING_STARTED_PX);
        let ids = &fixture["testIds"];
        assert_eq!(ids["home"].as_str().unwrap(), TEST_ID_HOME);
        assert_eq!(ids["greeting"].as_str().unwrap(), TEST_ID_GREETING);
        assert_eq!(ids["mark"].as_str().unwrap(), TEST_ID_MARK);
    }
}
