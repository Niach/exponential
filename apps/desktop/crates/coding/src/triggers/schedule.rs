//! Schedule occurrence math (EXP-530): pure, generic over the zone so
//! tests pin `FixedOffset` while hosts pass `chrono::Local` — a schedule
//! fires on the DEVICE's wall clock. DST posture: an ambiguous local time
//! (fall-back) takes `.earliest()`; a nonexistent one (spring-forward gap)
//! retries one hour later — the run fires once either way.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveTime, TimeZone};

use super::trigger::{Schedule, ScheduleInterval};

/// The latest occurrence at or before `now`, in `now`'s zone. `None` only
/// for a malformed schedule (out-of-range minute, weekly without weekday,
/// monthly without day) — never for a merely-distant one.
pub fn latest_occurrence<Tz: TimeZone>(
    schedule: &Schedule,
    now: DateTime<Tz>,
) -> Option<DateTime<Tz>> {
    occurrence(schedule, now, Direction::Latest)
}

/// The earliest occurrence strictly after `now` (the UI's "next run").
pub fn next_occurrence<Tz: TimeZone>(
    schedule: &Schedule,
    now: DateTime<Tz>,
) -> Option<DateTime<Tz>> {
    occurrence(schedule, now, Direction::Next)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Walk BACK from today for the newest candidate `<= now`.
    Latest,
    /// Walk FORWARD from today for the oldest candidate `> now`.
    Next,
}

fn occurrence<Tz: TimeZone>(
    schedule: &Schedule,
    now: DateTime<Tz>,
    direction: Direction,
) -> Option<DateTime<Tz>> {
    let tz = now.timezone();
    let today = now.date_naive();
    let accepts = |candidate: &DateTime<Tz>| match direction {
        Direction::Latest => *candidate <= now,
        Direction::Next => *candidate > now,
    };
    // Candidate dates walk outward from today; the ranges are one step
    // wider than the period so a today-candidate on the wrong side of `now`
    // (or one swallowed by a DST gap) always has a successor.
    let dates: Vec<NaiveDate> = match schedule.interval {
        ScheduleInterval::Daily => (0..3)
            .filter_map(|step| offset_days(today, step, direction))
            .collect(),
        ScheduleInterval::Weekly => {
            let weekday = schedule.weekday.filter(|day| (1..=7).contains(day))?;
            (0..15)
                .filter_map(|step| offset_days(today, step, direction))
                .filter(|date| date.weekday().number_from_monday() == weekday)
                .collect()
        }
        ScheduleInterval::Monthly => {
            let day = schedule.day_of_month.filter(|day| (1..=28).contains(day))?;
            (0..3)
                .filter_map(|step| {
                    let (year, month) = offset_months(today.year(), today.month(), step, direction);
                    // Days 1..=28 exist in every month.
                    NaiveDate::from_ymd_opt(year, month, day)
                })
                .collect()
        }
    };
    // The date walk is time-ordered per direction (Latest descends, Next
    // ascends), so the FIRST accepted candidate is the answer in both.
    dates
        .into_iter()
        .filter_map(|date| resolve(&tz, date, schedule.minute_of_day))
        .find(|candidate| accepts(candidate))
}

/// Resolve a naive local date+minute in `tz`. Ambiguous → `.earliest()`;
/// nonexistent (DST gap) → retry one hour later, `.earliest()` again.
fn resolve<Tz: TimeZone>(tz: &Tz, date: NaiveDate, minute_of_day: u32) -> Option<DateTime<Tz>> {
    let time = NaiveTime::from_hms_opt(minute_of_day / 60, minute_of_day % 60, 0)?;
    let naive = date.and_time(time);
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(resolved) => Some(resolved),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => tz.from_local_datetime(&(naive + Duration::hours(1))).earliest(),
    }
}

fn offset_days(today: NaiveDate, step: i64, direction: Direction) -> Option<NaiveDate> {
    match direction {
        Direction::Latest => today.checked_sub_signed(Duration::days(step)),
        Direction::Next => today.checked_add_signed(Duration::days(step)),
    }
}

fn offset_months(year: i32, month: u32, step: i32, direction: Direction) -> (i32, u32) {
    let delta = match direction {
        Direction::Latest => -step,
        Direction::Next => step,
    };
    let zero_based = year * 12 + (month as i32 - 1) + delta;
    (zero_based.div_euclid(12), (zero_based.rem_euclid(12) + 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn schedule(
        interval: ScheduleInterval,
        minute_of_day: u32,
        weekday: Option<u32>,
        day_of_month: Option<u32>,
    ) -> Schedule {
        Schedule {
            interval,
            minute_of_day,
            weekday,
            day_of_month,
        }
    }

    fn tz() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<FixedOffset> {
        tz().with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn daily_before_and_after_the_minute() {
        let daily = schedule(ScheduleInterval::Daily, 420, None, None); // 07:00
        // After today's minute: today is the latest, tomorrow the next.
        let now = at(2026, 8, 18, 8, 0);
        assert_eq!(latest_occurrence(&daily, now), Some(at(2026, 8, 18, 7, 0)));
        assert_eq!(next_occurrence(&daily, now), Some(at(2026, 8, 19, 7, 0)));
        // Before it: yesterday is the latest, today the next.
        let early = at(2026, 8, 18, 6, 0);
        assert_eq!(latest_occurrence(&daily, early), Some(at(2026, 8, 17, 7, 0)));
        assert_eq!(next_occurrence(&daily, early), Some(at(2026, 8, 18, 7, 0)));
        // Exactly at it: the occurrence counts as fired (<= now) and the
        // next is tomorrow (strictly after).
        let exact = at(2026, 8, 18, 7, 0);
        assert_eq!(latest_occurrence(&daily, exact), Some(exact));
        assert_eq!(next_occurrence(&daily, exact), Some(at(2026, 8, 19, 7, 0)));
    }

    #[test]
    fn weekly_wraps_across_the_week_boundary() {
        // 2026-08-18 is a Tuesday.
        let now = at(2026, 8, 18, 8, 0);
        // Monday 09:00: yesterday counts; next is the coming Monday.
        let monday = schedule(ScheduleInterval::Weekly, 540, Some(1), None);
        assert_eq!(latest_occurrence(&monday, now), Some(at(2026, 8, 17, 9, 0)));
        assert_eq!(next_occurrence(&monday, now), Some(at(2026, 8, 24, 9, 0)));
        // Wednesday 09:00: the latest wraps back a full week.
        let wednesday = schedule(ScheduleInterval::Weekly, 540, Some(3), None);
        assert_eq!(latest_occurrence(&wednesday, now), Some(at(2026, 8, 12, 9, 0)));
        assert_eq!(next_occurrence(&wednesday, now), Some(at(2026, 8, 19, 9, 0)));
        // Today's weekday only counts once its minute has passed.
        let tuesday = schedule(ScheduleInterval::Weekly, 420, Some(2), None);
        assert_eq!(latest_occurrence(&tuesday, now), Some(at(2026, 8, 18, 7, 0)));
        let before = at(2026, 8, 18, 6, 0);
        assert_eq!(latest_occurrence(&tuesday, before), Some(at(2026, 8, 11, 7, 0)));
        assert_eq!(next_occurrence(&tuesday, before), Some(at(2026, 8, 18, 7, 0)));
    }

    #[test]
    fn monthly_day_28_survives_short_months() {
        let monthly = schedule(ScheduleInterval::Monthly, 0, None, Some(28));
        // Early March: the latest is Feb 28 (2026 is no leap year — day 28
        // exists regardless), the next is Mar 28.
        let now = at(2026, 3, 5, 12, 0);
        assert_eq!(latest_occurrence(&monthly, now), Some(at(2026, 2, 28, 0, 0)));
        assert_eq!(next_occurrence(&monthly, now), Some(at(2026, 3, 28, 0, 0)));
        // January wraps the year boundary backwards.
        let january = at(2026, 1, 5, 12, 0);
        assert_eq!(latest_occurrence(&monthly, january), Some(at(2025, 12, 28, 0, 0)));
        // Day 5 mid-month: this month's occurrence already passed.
        let day5 = schedule(ScheduleInterval::Monthly, 540, None, Some(5));
        assert_eq!(latest_occurrence(&day5, now), Some(at(2026, 3, 5, 9, 0)));
        assert_eq!(next_occurrence(&day5, now), Some(at(2026, 4, 5, 9, 0)));
    }

    #[test]
    fn minute_bounds_render_midnight_and_2359() {
        let midnight = schedule(ScheduleInterval::Daily, 0, None, None);
        let now = at(2026, 8, 18, 12, 0);
        assert_eq!(latest_occurrence(&midnight, now), Some(at(2026, 8, 18, 0, 0)));
        let last_minute = schedule(ScheduleInterval::Daily, 1439, None, None);
        assert_eq!(
            latest_occurrence(&last_minute, now),
            Some(tz().with_ymd_and_hms(2026, 8, 17, 23, 59, 0).unwrap())
        );
        assert_eq!(
            next_occurrence(&last_minute, now),
            Some(tz().with_ymd_and_hms(2026, 8, 18, 23, 59, 0).unwrap())
        );
    }

    #[test]
    fn malformed_schedules_resolve_to_none() {
        let now = at(2026, 8, 18, 12, 0);
        // Weekly without a weekday / monthly without a day.
        assert_eq!(
            latest_occurrence(&schedule(ScheduleInterval::Weekly, 0, None, None), now),
            None
        );
        assert_eq!(
            latest_occurrence(&schedule(ScheduleInterval::Monthly, 0, None, None), now),
            None
        );
        // Out-of-range fields.
        assert_eq!(
            latest_occurrence(&schedule(ScheduleInterval::Daily, 1440, None, None), now),
            None
        );
        assert_eq!(
            next_occurrence(&schedule(ScheduleInterval::Weekly, 0, Some(8), None), now),
            None
        );
        assert_eq!(
            next_occurrence(&schedule(ScheduleInterval::Monthly, 0, None, Some(29)), now),
            None
        );
    }

    #[test]
    fn next_complements_latest() {
        // For every well-formed shape: latest <= now < next, and re-running
        // latest AT the next occurrence returns exactly it (no drift).
        let now = at(2026, 8, 18, 8, 30);
        for shape in [
            schedule(ScheduleInterval::Daily, 420, None, None),
            schedule(ScheduleInterval::Weekly, 540, Some(5), None),
            schedule(ScheduleInterval::Monthly, 60, None, Some(28)),
        ] {
            let latest = latest_occurrence(&shape, now).unwrap();
            let next = next_occurrence(&shape, now).unwrap();
            assert!(latest <= now, "{shape:?}");
            assert!(next > now, "{shape:?}");
            assert_eq!(latest_occurrence(&shape, next), Some(next), "{shape:?}");
        }
    }
}
