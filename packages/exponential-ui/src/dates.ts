// VAPP-190: calendar-day maths, shared by the records query (time buckets,
// host/query.ts) and the bind-time expressions (Q2: dateAdd, startOf,
// isoWeek…). Pure and proleptic Gregorian over `YYYY-MM-DD` texts, no Date
// object and no locale, so the Rust core's `dates` module mirrors it line
// by line. A date is a calendar day (spec §4.3); a timestamp's day is the
// first ten characters of its ISO text.

export interface CalendarDay {
  y: number
  m: number
  d: number
}

/** The time buckets a query groups dates by. */
export type DateBucket = `day` | `week` | `month` | `quarter` | `year`
export const DATE_BUCKETS: readonly DateBucket[] = [`day`, `week`, `month`, `quarter`, `year`]

const DIGIT = (c: string | undefined) => c !== undefined && c >= `0` && c <= `9`
const digits = (s: string, from: number, n: number) => {
  for (let i = from; i < from + n; i++) if (!DIGIT(s[i])) return false
  return true
}

export function isLeapYear(y: number): boolean {
  return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0
}

export function daysInMonth(y: number, m: number): number {
  return m === 2 ? (isLeapYear(y) ? 29 : 28) : m === 4 || m === 6 || m === 9 || m === 11 ? 30 : 31
}

/** 0 = Sunday (Sakamoto's method). */
export function weekdayOf(y: number, m: number, d: number): number {
  const t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4]
  const yy = m < 3 ? y - 1 : y
  const fl = (a: number, b: number) => Math.floor(a / b)
  return (((yy + fl(yy, 4) - fl(yy, 100) + fl(yy, 400) + t[m - 1]! + d) % 7) + 7) % 7
}

/** 1-based day of the year. */
export function dayOfYear(y: number, m: number, d: number): number {
  let n = d
  for (let i = 1; i < m; i++) n += daysInMonth(y, i)
  return n
}

/** ISO 8601 weeks in a year: 53 when it starts on a Thursday (or a Wednesday in a leap year). */
export function isoWeeksInYear(y: number): number {
  const jan1 = weekdayOf(y, 1, 1)
  return jan1 === 4 || (jan1 === 3 && isLeapYear(y)) ? 53 : 52
}

/** The ISO week (Monday first, week 1 holds the first Thursday) and its year. */
export function isoWeek(y: number, m: number, d: number): { year: number; week: number } {
  const wd = weekdayOf(y, m, d) || 7
  const week = Math.floor((dayOfYear(y, m, d) - wd + 10) / 7)
  if (week < 1) return { year: y - 1, week: isoWeeksInYear(y - 1) }
  if (week > isoWeeksInYear(y)) return { year: y + 1, week: 1 }
  return { year: y, week }
}

/** The calendar day at the start of `text` (`YYYY-MM-DD`, then the end, `T`
 *  or a space); null when it is not a real day. */
export function parseDay(text: string): CalendarDay | null {
  if (text.length < 10 || !digits(text, 0, 4) || text[4] !== `-` || !digits(text, 5, 2) || text[7] !== `-` || !digits(text, 8, 2)) return null
  if (text.length > 10 && text[10] !== `T` && text[10] !== ` `) return null
  const y = Number(text.slice(0, 4))
  const m = Number(text.slice(5, 7))
  const d = Number(text.slice(8, 10))
  if (m < 1 || m > 12 || d < 1 || d > daysInMonth(y, m)) return null
  return { y, m, d }
}

/** The year and month at the start of `text` (`YYYY-MM`, then the end or `-`). */
export function parseMonth(text: string): { y: number; m: number } | null {
  if (text.length < 7 || !digits(text, 0, 4) || text[4] !== `-` || !digits(text, 5, 2)) return null
  if (text.length > 7 && text[7] !== `-`) return null
  const m = Number(text.slice(5, 7))
  return m >= 1 && m <= 12 ? { y: Number(text.slice(0, 4)), m } : null
}

/** The year at the start of `text` (`YYYY`, then the end or `-`). */
export function parseYear(text: string): number | null {
  if (text.length < 4 || !digits(text, 0, 4) || (text.length > 4 && text[4] !== `-`)) return null
  return Number(text.slice(0, 4))
}

const pad = (n: number, w: number) => String(n).padStart(w, `0`)

/** `YYYY-MM-DD`. */
export function formatDay(day: CalendarDay): string {
  return `${pad(day.y, 4)}-${pad(day.m, 2)}-${pad(day.d, 2)}`
}

/** A date's bucket key: day `2026-10-05`, week `2026-W41` (ISO), month
 *  `2026-10`, quarter `2026-Q4`, year `2026`; null when the text holds no
 *  date that precise (a week or a day needs a whole day, a month or a
 *  quarter `YYYY-MM`, a year `YYYY`). */
export function dateBucket(text: string, bucket: DateBucket): string | null {
  if (bucket === `day` || bucket === `week`) {
    const day = parseDay(text)
    if (!day) return null
    if (bucket === `day`) return formatDay(day)
    const w = isoWeek(day.y, day.m, day.d)
    return `${pad(w.year, 4)}-W${pad(w.week, 2)}`
  }
  if (bucket === `year`) {
    const y = parseYear(text)
    return y === null ? null : pad(y, 4)
  }
  const month = parseMonth(text)
  if (!month) return null
  return bucket === `month` ? `${pad(month.y, 4)}-${pad(month.m, 2)}` : `${pad(month.y, 4)}-Q${Math.floor((month.m - 1) / 3) + 1}`
}

/** VAPP-191: the days since 1970-01-01 of a calendar day (negative before;
 *  Howard Hinnant's days_from_civil), and back. */
export function dayNumber(y: number, m: number, d: number): number {
  const yy = m <= 2 ? y - 1 : y
  const era = Math.floor(yy / 400)
  const yoe = yy - era * 400
  const doy = Math.floor((153 * (m + (m > 2 ? -3 : 9)) + 2) / 5) + d - 1
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy
  return era * 146097 + doe - 719468
}

export function dayFromNumber(n: number): CalendarDay {
  const z = n + 719468
  const era = Math.floor(z / 146097)
  const doe = z - era * 146097
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365)
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100))
  const mp = Math.floor((5 * doy + 2) / 153)
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1
  const m = mp < 10 ? mp + 3 : mp - 9
  return { y: yoe + era * 400 + (m <= 2 ? 1 : 0), m, d }
}
