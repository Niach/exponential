// NEW-2: THE schedule picker's pure core ×4 — nobody speaks cron. A schedule
// trigger stores a 5-field cron; these rows edit it and `scheduleSummary`
// reads it. Byte-locked by `fixtures/schedule-picker.json` (`cronCases`,
// `parseCases`, `weekdaysLabelCases`, `summaryCases`), which every mirror
// replays: desktop `domain::schedule_picker`, iOS ExpCore
// `SchedulePicker.swift`, Android `domain/SchedulePicker.kt`. The fixture's
// `$comment` holds the rules.

import fixture from "../fixtures/schedule-picker.json" with { type: "json" }

export const SCHEDULE_PICKER_COPY = fixture.copy
export const SCHEDULE_PICKER_TEST_IDS = fixture.testIds
export const SCHEDULE_REPEAT_VALUES = fixture.repeatValues as readonly ScheduleRepeat[]
export const SCHEDULE_DAY_OF_MONTH_MAX: number = fixture.dayOfMonthMax
/** EXP-1363: the `minutes` repeat's intervals. */
export const SCHEDULE_EVERY_VALUES: readonly number[] = fixture.everyValues

export type ScheduleRepeat = `minutes` | `hourly` | `daily` | `weekly` | `monthly`

/** What the rows edit; `cronFromSchedule` is its wire form. */
export interface Schedule {
  repeat: ScheduleRepeat
  /** EXP-1363: one of `SCHEDULE_EVERY_VALUES`; minutes only. */
  every: number
  /** 0..59; unused by `minutes`. */
  minute: number
  /** 0..23; unused by `hourly`. */
  hour: number
  /** ISO weekdays 1 = Monday .. 7 = Sunday, ascending, unique; weekly only. */
  weekdays: number[]
  /** 1..28; monthly only. */
  dayOfMonth: number
}

/** A new schedule: daily at 09:00 (the fixture's `defaults`). */
export function defaultSchedule(): Schedule {
  const d = fixture.defaults
  return {
    repeat: d.repeat as ScheduleRepeat,
    every: d.every,
    minute: d.minute,
    hour: d.hour,
    weekdays: [...d.weekdays],
    dayOfMonth: d.dayOfMonth,
  }
}

const pad2 = (value: number): string => String(value).padStart(2, `0`)

/** The 24h `HH:mm` a Time row reads. */
export function scheduleTimeLabel(hour: number, minute: number): string {
  return `${pad2(hour)}:${pad2(minute)}`
}

/** ISO weekday (1 = Monday .. 7 = Sunday) → cron's (Sunday = 0). */
const toCronWeekday = (iso: number): number => (iso === 7 ? 0 : iso)
/** cron weekday (0 or 7 = Sunday) → ISO. */
const fromCronWeekday = (cron: number): number => (cron === 0 ? 7 : cron)

/** The canonical cron of a schedule (the fixture's `cronCases`). */
export function cronFromSchedule(schedule: Schedule): string {
  const minute = String(schedule.minute)
  switch (schedule.repeat) {
    case `minutes`:
      return `*/${schedule.every} * * * *`
    case `hourly`:
      return `${minute} * * * *`
    case `daily`:
      return `${minute} ${schedule.hour} * * *`
    case `weekly`: {
      const days = [...new Set(schedule.weekdays.map(toCronWeekday))].sort((a, b) => a - b)
      return `${minute} ${schedule.hour} * * ${days.join(`,`)}`
    }
    case `monthly`:
      return `${minute} ${schedule.hour} ${schedule.dayOfMonth} * *`
  }
}

/** A single plain number within [min, max], else null (no lists, no `*`). */
function plainNumber(field: string, min: number, max: number): number | null {
  if (!/^\d{1,2}$/.test(field)) return null
  const value = Number(field)
  return value >= min && value <= max ? value : null
}

/** A cron weekday field as ISO weekdays: plain numbers, lists and ranges of
 *  0..7 (0 and 7 = Sunday), any order; null for anything else. */
function weekdaysOf(field: string): number[] | null {
  const out = new Set<number>()
  for (const part of field.split(`,`)) {
    const range = /^(\d)(?:-(\d))?$/.exec(part)
    if (!range) return null
    const lo = Number(range[1])
    const hi = range[2] === undefined ? lo : Number(range[2])
    if (lo > 7 || hi > 7 || lo > hi) return null
    for (let day = lo; day <= hi; day += 1) out.add(fromCronWeekday(day))
  }
  return [...out].sort((a, b) => a - b)
}

/** The schedule a cron reads as, or null when the rows cannot express it
 *  (a CUSTOM schedule; the fixture's `parseCases`). */
export function scheduleFromCron(cron: string): Schedule | null {
  const fields = cron.trim().split(/\s+/)
  if (fields.length !== 5 || fields[0] === ``) return null
  const [minuteField, hourField, domField, monthField, dowField] = fields as [
    string,
    string,
    string,
    string,
    string,
  ]
  if (monthField !== `*`) return null
  const step = /^\*\/(\d{1,2})$/.exec(minuteField)
  if (step) {
    const every = Number(step[1])
    const plain = hourField === `*` && domField === `*` && dowField === `*`
    return plain && SCHEDULE_EVERY_VALUES.includes(every)
      ? { repeat: `minutes`, every, minute: 0, hour: 0, weekdays: [], dayOfMonth: 1 }
      : null
  }
  const minute = plainNumber(minuteField, 0, 59)
  if (minute === null) return null
  // Only the repeat's own fields are populated: `weekdays` [] unless weekly,
  // `dayOfMonth` 1 unless monthly (the rows seed `defaults` on a switch).
  const base: Omit<Schedule, `repeat` | `hour`> = { every: fixture.defaults.every, minute, weekdays: [], dayOfMonth: 1 }
  if (hourField === `*`) {
    return domField === `*` && dowField === `*` ? { ...base, repeat: `hourly`, hour: 0 } : null
  }
  const hour = plainNumber(hourField, 0, 23)
  if (hour === null) return null
  if (domField === `*` && dowField === `*`) return { ...base, repeat: `daily`, hour }
  if (domField === `*`) {
    const weekdays = weekdaysOf(dowField)
    return weekdays && weekdays.length > 0 ? { ...base, repeat: `weekly`, hour, weekdays } : null
  }
  if (dowField !== `*`) return null
  const dayOfMonth = plainNumber(domField, 1, SCHEDULE_DAY_OF_MONTH_MAX)
  return dayOfMonth === null ? null : { ...base, repeat: `monthly`, hour, dayOfMonth }
}

/** The Days row's value and the weekly summary's `{days}`. */
export function weekdaysLabel(weekdays: readonly number[]): string {
  const copy = fixture.copy
  const days = [...new Set(weekdays)].sort((a, b) => a - b)
  const is = (expected: number[]) => days.length === expected.length && days.every((d, i) => d === expected[i])
  if (days.length === 0) return ``
  if (is([1, 2, 3, 4, 5, 6, 7])) return copy.weekdaysAll
  if (is([1, 2, 3, 4, 5])) return copy.weekdaysWork
  if (is([6, 7])) return copy.weekdaysWeekend
  if (days.length === 1) return copy.weekdays[days[0]! - 1] ?? ``
  return days.map((day) => copy.weekdaysShort[day - 1] ?? ``).join(`, `)
}

/** A Triggers row's text for a stored schedule (the fixture's `summaryCases`). */
export function scheduleSummary(cron: string, tz?: string | null): string {
  const copy = fixture.copy.summary
  const schedule = scheduleFromCron(cron)
  let text: string
  if (!schedule) {
    text = copy.custom.replace(`{cron}`, cron.trim().replace(/\s+/g, ` `))
  } else {
    const time = scheduleTimeLabel(schedule.hour, schedule.minute)
    switch (schedule.repeat) {
      case `minutes`:
        return copy.minutes.replace(`{n}`, String(schedule.every))
      case `hourly`:
        // EXP-1363: a zone means nothing for an hourly schedule.
        return copy.hourly.replace(`{minute}`, pad2(schedule.minute))
      case `daily`:
        text = copy.daily.replace(`{time}`, time)
        break
      case `weekly`:
        text =
          schedule.weekdays.length === 1
            ? copy.weeklyOne.replace(`{weekday}`, weekdaysLabel(schedule.weekdays)).replace(`{time}`, time)
            : copy.weekly.replace(`{days}`, weekdaysLabel(schedule.weekdays)).replace(`{time}`, time)
        break
      case `monthly`:
        text = copy.monthly.replace(`{day}`, String(schedule.dayOfMonth)).replace(`{time}`, time)
        break
    }
  }
  return tz ? copy.zoned.replace(`{summary}`, text).replace(`{tz}`, tz) : text
}
