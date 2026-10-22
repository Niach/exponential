//! NEW-2: THE schedule picker's pure core ×4 — nobody speaks cron. The Rust
//! twin of `packages/domain-contract/src/schedule-picker.ts` (iOS ExpCore
//! `SchedulePicker.swift`, Android `domain/SchedulePicker.kt`), byte-locked
//! by `fixtures/schedule-picker.json` (`cronCases`, `parseCases`,
//! `weekdaysLabelCases`, `summaryCases`). A schedule trigger STORES a 5-field
//! cron; the rows (`ui::schedule_fields`) edit it, [`schedule_summary`]
//! reads it. The fixture's `$comment` holds the rules.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRowsCopy {
    pub repeat: String,
    pub days: String,
    pub day: String,
    pub minute: String,
    pub time: String,
    pub time_zone: String,
    pub cron: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScheduleRepeatCopy {
    pub hourly: String,
    pub daily: String,
    pub weekly: String,
    pub monthly: String,
    pub custom: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSummaryCopy {
    pub hourly: String,
    pub daily: String,
    pub weekly: String,
    pub weekly_one: String,
    pub monthly: String,
    pub custom: String,
    pub zoned: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleErrorCopy {
    pub no_days: String,
    pub cron: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePickerCopy {
    pub rows: ScheduleRowsCopy,
    pub repeat: ScheduleRepeatCopy,
    /// Monday .. Sunday.
    pub weekdays: Vec<String>,
    pub weekdays_short: Vec<String>,
    pub weekdays_all: String,
    pub weekdays_work: String,
    pub weekdays_weekend: String,
    /// `Day {day}`.
    pub day_of_month: String,
    /// `:{minute}`.
    pub minute_of_hour: String,
    pub time_zone_placeholder: String,
    pub cron_placeholder: String,
    pub summary: ScheduleSummaryCopy,
    pub errors: ScheduleErrorCopy,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePickerTestIds {
    pub repeat: String,
    pub days: String,
    pub day: String,
    pub minute: String,
    pub time: String,
    pub time_zone: String,
    pub cron: String,
}

/// What the rows edit; [`cron_from_schedule`] is its wire form.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub repeat: ScheduleRepeat,
    /// 0..59.
    pub minute: u32,
    /// 0..23; unused by hourly.
    pub hour: u32,
    /// ISO weekdays 1 = Monday .. 7 = Sunday, ascending, unique; weekly only.
    pub weekdays: Vec<u32>,
    /// 1..=28; monthly only.
    pub day_of_month: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleRepeat {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl ScheduleRepeat {
    pub const ALL: [ScheduleRepeat; 4] = [Self::Hourly, Self::Daily, Self::Weekly, Self::Monthly];

    /// The wire / fixture value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|repeat| repeat.as_str() == value)
    }

    /// The Repeat row's label (`copy.repeat`).
    pub fn label(self) -> &'static str {
        let copy = &SCHEDULE_PICKER.copy.repeat;
        match self {
            Self::Hourly => &copy.hourly,
            Self::Daily => &copy.daily,
            Self::Weekly => &copy.weekly,
            Self::Monthly => &copy.monthly,
        }
    }
}

/// `fixtures/schedule-picker.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePickerSpec {
    pub repeat_values: Vec<ScheduleRepeat>,
    pub day_of_month_max: u32,
    pub defaults: Schedule,
    pub copy: SchedulePickerCopy,
    pub test_ids: SchedulePickerTestIds,
}

const SCHEDULE_PICKER_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/schedule-picker.json");

/// The picker's copy, defaults and test ids.
pub static SCHEDULE_PICKER: LazyLock<SchedulePickerSpec> = LazyLock::new(|| {
    serde_json::from_str(SCHEDULE_PICKER_JSON).expect("schedule-picker.json parses")
});

/// A new schedule: daily at 09:00 (the fixture's `defaults`).
pub fn default_schedule() -> Schedule {
    SCHEDULE_PICKER.defaults.clone()
}

/// The 24h `HH:mm` a Time row reads.
pub fn time_label(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

/// ISO weekday (7 = Sunday) → cron's (Sunday = 0).
fn to_cron_weekday(iso: u32) -> u32 {
    if iso == 7 { 0 } else { iso }
}

/// The canonical cron of a schedule (the fixture's `cronCases`).
pub fn cron_from_schedule(schedule: &Schedule) -> String {
    let minute = schedule.minute;
    let hour = schedule.hour;
    match schedule.repeat {
        ScheduleRepeat::Hourly => format!("{minute} * * * *"),
        ScheduleRepeat::Daily => format!("{minute} {hour} * * *"),
        ScheduleRepeat::Weekly => {
            let days: BTreeSet<u32> = schedule.weekdays.iter().map(|day| to_cron_weekday(*day)).collect();
            let days: Vec<String> = days.iter().map(u32::to_string).collect();
            format!("{minute} {hour} * * {}", days.join(","))
        }
        ScheduleRepeat::Monthly => format!("{minute} {hour} {} * *", schedule.day_of_month),
    }
}

/// A single plain number (1-2 digits) within [min, max].
fn plain_number(field: &str, min: u32, max: u32) -> Option<u32> {
    if field.is_empty() || field.len() > 2 || !field.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: u32 = field.parse().ok()?;
    (min..=max).contains(&value).then_some(value)
}

/// A cron weekday field as ISO weekdays: plain numbers, lists and ranges of
/// 0..7 (0 and 7 = Sunday), any order; `None` for anything else.
fn weekdays_of(field: &str) -> Option<Vec<u32>> {
    let digit = |part: &str| -> Option<u32> {
        (part.len() == 1 && part.as_bytes()[0].is_ascii_digit()).then(|| u32::from(part.as_bytes()[0] - b'0'))
    };
    let mut out = BTreeSet::new();
    for part in field.split(',') {
        let (lo, hi) = match part.split_once('-') {
            Some((lo, hi)) => (digit(lo)?, digit(hi)?),
            None => {
                let day = digit(part)?;
                (day, day)
            }
        };
        if lo > 7 || hi > 7 || lo > hi {
            return None;
        }
        for day in lo..=hi {
            out.insert(if day == 0 { 7 } else { day });
        }
    }
    Some(out.into_iter().collect())
}

/// The schedule a cron reads as, or `None` when the rows cannot express it
/// (a CUSTOM schedule; the fixture's `parseCases`).
pub fn schedule_from_cron(cron: &str) -> Option<Schedule> {
    let fields: Vec<&str> = cron.split_whitespace().collect();
    let [minute_field, hour_field, dom_field, month_field, dow_field] = fields.as_slice() else {
        return None;
    };
    if *month_field != "*" {
        return None;
    }
    let minute = plain_number(minute_field, 0, 59)?;
    // Only the repeat's own fields are populated: `weekdays` [] unless
    // weekly, `day_of_month` 1 unless monthly.
    let base = |repeat, hour| Schedule {
        repeat,
        minute,
        hour,
        weekdays: Vec::new(),
        day_of_month: 1,
    };
    if *hour_field == "*" {
        return (*dom_field == "*" && *dow_field == "*").then(|| base(ScheduleRepeat::Hourly, 0));
    }
    let hour = plain_number(hour_field, 0, 23)?;
    if *dom_field == "*" && *dow_field == "*" {
        return Some(base(ScheduleRepeat::Daily, hour));
    }
    if *dom_field == "*" {
        let weekdays = weekdays_of(dow_field).filter(|days| !days.is_empty())?;
        return Some(Schedule {
            weekdays,
            ..base(ScheduleRepeat::Weekly, hour)
        });
    }
    if *dow_field != "*" {
        return None;
    }
    let day_of_month = plain_number(dom_field, 1, SCHEDULE_PICKER.day_of_month_max)?;
    Some(Schedule {
        day_of_month,
        ..base(ScheduleRepeat::Monthly, hour)
    })
}

/// The Days row's value and the weekly summary's `{days}`.
pub fn weekdays_label(weekdays: &[u32]) -> String {
    let copy = &SCHEDULE_PICKER.copy;
    let days: Vec<u32> = weekdays.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
    let name = |list: &[String], day: u32| list.get(day as usize - 1).cloned().unwrap_or_default();
    match days.as_slice() {
        [] => String::new(),
        [1, 2, 3, 4, 5, 6, 7] => copy.weekdays_all.clone(),
        [1, 2, 3, 4, 5] => copy.weekdays_work.clone(),
        [6, 7] => copy.weekdays_weekend.clone(),
        [day] => name(&copy.weekdays, *day),
        _ => days
            .iter()
            .map(|day| name(&copy.weekdays_short, *day))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// A Triggers row's text for a stored schedule (the fixture's
/// `summaryCases`); a set zone wraps it in `copy.summary.zoned`.
pub fn schedule_summary(cron: &str, tz: Option<&str>) -> String {
    let copy = &SCHEDULE_PICKER.copy.summary;
    let text = match schedule_from_cron(cron) {
        None => copy
            .custom
            .replacen("{cron}", &cron.split_whitespace().collect::<Vec<_>>().join(" "), 1),
        Some(schedule) => {
            let time = time_label(schedule.hour, schedule.minute);
            match schedule.repeat {
                ScheduleRepeat::Hourly => copy.hourly.replacen("{minute}", &format!("{:02}", schedule.minute), 1),
                ScheduleRepeat::Daily => copy.daily.replacen("{time}", &time, 1),
                ScheduleRepeat::Weekly if schedule.weekdays.len() == 1 => copy
                    .weekly_one
                    .replacen("{weekday}", &weekdays_label(&schedule.weekdays), 1)
                    .replacen("{time}", &time, 1),
                ScheduleRepeat::Weekly => copy
                    .weekly
                    .replacen("{days}", &weekdays_label(&schedule.weekdays), 1)
                    .replacen("{time}", &time, 1),
                ScheduleRepeat::Monthly => copy
                    .monthly
                    .replacen("{day}", &schedule.day_of_month.to_string(), 1)
                    .replacen("{time}", &time, 1),
            }
        }
    };
    match tz.filter(|tz| !tz.is_empty()) {
        Some(tz) => copy.zoned.replacen("{summary}", &text, 1).replacen("{tz}", tz, 1),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(SCHEDULE_PICKER_JSON).unwrap()
    }

    fn schedule_of(value: &Value) -> Option<Schedule> {
        serde_json::from_value(value.clone()).ok()
    }

    #[test]
    fn the_spec_parses_from_the_fixture() {
        assert_eq!(SCHEDULE_PICKER.repeat_values, ScheduleRepeat::ALL);
        for repeat in ScheduleRepeat::ALL {
            assert!(!repeat.label().is_empty());
            assert_eq!(ScheduleRepeat::parse(repeat.as_str()), Some(repeat));
        }
        assert!(!SCHEDULE_PICKER.copy.repeat.custom.is_empty());
        assert_eq!(SCHEDULE_PICKER.copy.weekdays.len(), 7);
        assert_eq!(SCHEDULE_PICKER.copy.weekdays_short.len(), 7);
        assert_eq!(SCHEDULE_PICKER.day_of_month_max, 28);
    }

    #[test]
    fn the_defaults_are_daily_at_nine() {
        assert_eq!(cron_from_schedule(&default_schedule()), "0 9 * * *");
    }

    #[test]
    fn the_time_zone_list_starts_with_utc() {
        let names = crate::contract::TIME_ZONE_NAMES;
        assert_eq!(names[0], "UTC");
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
    }

    #[test]
    fn cron_cases_round_trip() {
        let fixture = fixture();
        let cases = fixture["cronCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let schedule = schedule_of(&case["schedule"]).unwrap();
            let cron = case["cron"].as_str().unwrap();
            assert_eq!(cron_from_schedule(&schedule), cron, "{}", case["name"]);
            assert_eq!(schedule_from_cron(cron), Some(schedule), "{}", case["name"]);
        }
    }

    #[test]
    fn parse_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["parseCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let cron = case["cron"].as_str().unwrap();
            let expected = if case["schedule"].is_null() { None } else { Some(schedule_of(&case["schedule"]).unwrap()) };
            assert_eq!(schedule_from_cron(cron), expected, "{cron:?}");
        }
    }

    #[test]
    fn weekdays_label_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["weekdaysLabelCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let weekdays: Vec<u32> = serde_json::from_value(case["weekdays"].clone()).unwrap();
            assert_eq!(weekdays_label(&weekdays), case["expected"].as_str().unwrap(), "{weekdays:?}");
        }
    }

    #[test]
    fn summary_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["summaryCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let cron = case["cron"].as_str().unwrap();
            assert_eq!(
                schedule_summary(cron, case["tz"].as_str()),
                case["expected"].as_str().unwrap(),
                "{cron:?}"
            );
        }
    }
}
