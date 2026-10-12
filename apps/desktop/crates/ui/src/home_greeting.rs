//! EXP-1253 (EXP-1259 D1) — the Agent home's greeting ("Evening, Danny"),
//! the desktop mirror of `packages/domain-contract/src/home-greeting.ts`.
//! Pure and byte-locked by `fixtures/composer-home.json`: the test below
//! replays its `cases` and pins the copy + geometry consts to it. The
//! fixture's `$comment` holds the rules.

use chrono::{NaiveDate, Timelike as _};

/// The composer's placeholder with no subject picked.
pub(crate) const HOME_PLACEHOLDER: &str = "What do you want to build?";
/// The Recent band's label and how many runs it lists.
pub(crate) const RECENT_LABEL: &str = "Recent";
pub(crate) const RECENT_COUNT: usize = 3;

/// The fixture's `testIds`: the IDE has no DOM, so they name the home's
/// element ids (a capture or a test finds the same pieces as on the web).
pub(crate) const TEST_ID_HOME: &str = "agent-home";
pub(crate) const TEST_ID_GREETING: &str = "agent-home-greeting";
pub(crate) const TEST_ID_MARK: &str = "agent-home-mark";
pub(crate) const TEST_ID_RECENT: &str = "agent-home-recent";
pub(crate) const TEST_ID_RECENT_ROW: &str = "agent-home-recent-row";

/// The headline row's geometry (px): the disc mark, the gap to the
/// greeting, the greeting's size + weight, then the gaps to the card and
/// from the options line to Recent.
pub(crate) const MARK_PX: f32 = 32.;
pub(crate) const GAP_PX: f32 = 12.;
pub(crate) const HEADLINE_PX: f32 = 30.;
pub(crate) const HEADLINE_WEIGHT: f32 = 600.;
pub(crate) const HEADLINE_TO_CARD_PX: f32 = 24.;
pub(crate) const OPTIONS_TO_RECENT_PX: f32 = 32.;

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

/// A row stamp as epoch milliseconds (unparseable = 0, sorts last).
#[cfg_attr(not(test), allow(dead_code))]
fn stamp_ms(stamp: Option<&str>) -> i64 {
    stamp
        .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
        .map(|at| at.timestamp_millis())
        .unwrap_or(0)
}

/// RECENT: the caller's last [`RECENT_COUNT`] OWN top-level runs (no parent
/// run) in `team_id`, any status, newest first by `updated_at` then
/// `created_at`. EXP-1284: the home lists THREADS now
/// (`threads::team_thread_lists`); the run rule stays locked by its tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn home_recent_runs<'a>(
    rows: impl Iterator<Item = &'a domain::rows::CodingSession>,
    me: &str,
    team_id: &str,
) -> Vec<&'a domain::rows::CodingSession> {
    let mut out: Vec<&domain::rows::CodingSession> = rows
        .filter(|row| row.user_id.as_deref() == Some(me))
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .filter(|row| row.parent_session_id.is_none())
        .collect();
    out.sort_by(|a, b| {
        stamp_ms(b.updated_at.as_deref())
            .cmp(&stamp_ms(a.updated_at.as_deref()))
            .then_with(|| stamp_ms(b.created_at.as_deref()).cmp(&stamp_ms(a.created_at.as_deref())))
            .then_with(|| b.id.cmp(&a.id))
    });
    out.truncate(RECENT_COUNT);
    out
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
        assert_eq!(fixture["recent"]["label"].as_str().unwrap(), RECENT_LABEL);
        assert_eq!(fixture["recent"]["count"].as_u64().unwrap() as usize, RECENT_COUNT);
        let g = &fixture["geometry"];
        let px = |key: &str| g[key].as_f64().unwrap() as f32;
        assert_eq!(px("mark"), MARK_PX);
        assert_eq!(px("gap"), GAP_PX);
        assert_eq!(px("headline"), HEADLINE_PX);
        assert_eq!(px("headlineWeight"), HEADLINE_WEIGHT);
        assert_eq!(px("headlineToCard"), HEADLINE_TO_CARD_PX);
        assert_eq!(px("optionsToRecent"), OPTIONS_TO_RECENT_PX);
        let ids = &fixture["testIds"];
        assert_eq!(ids["home"].as_str().unwrap(), TEST_ID_HOME);
        assert_eq!(ids["greeting"].as_str().unwrap(), TEST_ID_GREETING);
        assert_eq!(ids["mark"].as_str().unwrap(), TEST_ID_MARK);
        assert_eq!(ids["recent"].as_str().unwrap(), TEST_ID_RECENT);
        assert_eq!(ids["recentRow"].as_str().unwrap(), TEST_ID_RECENT_ROW);
    }

    fn run(id: &str, user: &str, team: &str, parent: Option<&str>, updated: &str) -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "user_id": user,
            "team_id": team,
            "parent_session_id": parent,
            "updated_at": updated,
        }))
        .expect("a coding session row")
    }

    #[test]
    fn recent_is_my_last_three_top_level_runs_in_the_team() {
        let rows = vec![
            run("a", "me", "t", None, "2026-10-12T08:00:00Z"),
            run("b", "me", "t", None, "2026-10-12T09:00:00Z"),
            run("child", "me", "t", Some("b"), "2026-10-12T12:00:00Z"),
            run("other-user", "you", "t", None, "2026-10-12T12:00:00Z"),
            run("other-team", "me", "u", None, "2026-10-12T12:00:00Z"),
            run("c", "me", "t", None, "2026-10-12T10:00:00Z"),
            run("d", "me", "t", None, "2026-10-12T07:00:00Z"),
        ];
        let ids: Vec<&str> = home_recent_runs(rows.iter(), "me", "t")
            .into_iter()
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(ids, ["c", "b", "a"]);
        assert!(home_recent_runs(rows.iter(), "nobody", "t").is_empty());
    }
}
