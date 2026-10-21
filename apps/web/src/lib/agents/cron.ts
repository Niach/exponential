// NEW-2: a 5-field cron (`minute hour day-of-month month day-of-week`) with
// minute precision, read in an IANA time zone. `*`, lists (`1,15`), ranges
// (`9-17`), steps (`*/10`, `1-5/2`), names for months and weekdays, `7` =
// Sunday. Like vixie cron, a restricted day-of-month AND a restricted
// day-of-week match when EITHER does. Pure: the watchdog asks for the latest
// matching minute inside a window (`latestCronMatch`).

export interface CronFields {
  minute: Set<number>
  hour: Set<number>
  dom: Set<number>
  month: Set<number>
  dow: Set<number>
  /** `*` (any day) on the respective field: the vixie OR rule needs it. */
  anyDom: boolean
  anyDow: boolean
}

const MONTHS = [`jan`, `feb`, `mar`, `apr`, `may`, `jun`, `jul`, `aug`, `sep`, `oct`, `nov`, `dec`]
const WEEKDAYS = [`sun`, `mon`, `tue`, `wed`, `thu`, `fri`, `sat`]

function parseField(
  raw: string,
  min: number,
  max: number,
  names: readonly string[] | null
): Set<number> | null {
  const out = new Set<number>()
  const atom = (token: string): number | null => {
    const lower = token.toLowerCase()
    const named = names ? names.indexOf(lower) : -1
    if (named >= 0) return named + (names === MONTHS ? 1 : 0)
    if (!/^\d+$/.test(token)) return null
    const n = Number(token)
    return Number.isInteger(n) ? n : null
  }
  for (const part of raw.split(`,`)) {
    if (part === ``) return null
    const [rangePart, stepPart, ...extra] = part.split(`/`)
    if (extra.length > 0) return null
    let step = 1
    if (stepPart !== undefined) {
      if (!/^\d+$/.test(stepPart) || Number(stepPart) < 1) return null
      step = Number(stepPart)
    }
    let lo: number
    let hi: number
    if (rangePart === `*`) {
      lo = min
      hi = max
    } else if (rangePart!.includes(`-`)) {
      const [a, b, ...rest] = rangePart!.split(`-`)
      if (rest.length > 0 || a === undefined || b === undefined) return null
      const av = atom(a)
      const bv = atom(b)
      if (av === null || bv === null) return null
      lo = av
      hi = bv
    } else {
      const v = atom(rangePart!)
      if (v === null) return null
      lo = v
      hi = stepPart !== undefined ? max : v
    }
    if (lo < min || hi > max || lo > hi) return null
    for (let v = lo; v <= hi; v += step) out.add(v)
  }
  return out
}

/** Parses an expression; null when it is not a valid 5-field cron. */
export function parseCron(expression: string): CronFields | null {
  const parts = expression.trim().split(/\s+/)
  if (parts.length !== 5) return null
  const [m, h, dom, mon, dow] = parts as [string, string, string, string, string]
  const minute = parseField(m, 0, 59, null)
  const hour = parseField(h, 0, 23, null)
  const domSet = parseField(dom, 1, 31, null)
  const month = parseField(mon, 1, 12, MONTHS)
  const dowSet = parseField(dow, 0, 7, WEEKDAYS)
  if (!minute || !hour || !domSet || !month || !dowSet) return null
  if (dowSet.has(7)) {
    dowSet.delete(7)
    dowSet.add(0)
  }
  return {
    minute,
    hour,
    dom: domSet,
    month,
    dow: dowSet,
    anyDom: dom === `*`,
    anyDow: dow === `*`,
  }
}

export interface WallClock {
  minute: number
  hour: number
  dom: number
  month: number
  dow: number
}

const formatters = new Map<string, Intl.DateTimeFormat>()

/** The wall clock of `date` in `tz` (an invalid zone reads as UTC). */
export function wallClock(date: Date, tz: string | null | undefined): WallClock {
  const zone = tz || `UTC`
  let formatter = formatters.get(zone)
  if (!formatter) {
    try {
      formatter = new Intl.DateTimeFormat(`en-US`, {
        timeZone: zone,
        hourCycle: `h23`,
        minute: `numeric`,
        hour: `numeric`,
        day: `numeric`,
        month: `numeric`,
        weekday: `short`,
      })
    } catch {
      formatter = new Intl.DateTimeFormat(`en-US`, {
        timeZone: `UTC`,
        hourCycle: `h23`,
        minute: `numeric`,
        hour: `numeric`,
        day: `numeric`,
        month: `numeric`,
        weekday: `short`,
      })
    }
    formatters.set(zone, formatter)
  }
  const parts: Record<string, string> = {}
  for (const part of formatter.formatToParts(date)) parts[part.type] = part.value
  return {
    minute: Number(parts.minute),
    hour: Number(parts.hour) % 24,
    dom: Number(parts.day),
    month: Number(parts.month),
    dow: WEEKDAYS.indexOf((parts.weekday ?? `sun`).toLowerCase().slice(0, 3)),
  }
}

/** Whether the minute `date` falls in matches the expression in `tz`. */
export function cronMatches(fields: CronFields, date: Date, tz: string | null | undefined): boolean {
  const clock = wallClock(date, tz)
  if (!fields.minute.has(clock.minute)) return false
  if (!fields.hour.has(clock.hour)) return false
  if (!fields.month.has(clock.month)) return false
  const domOk = fields.dom.has(clock.dom)
  const dowOk = fields.dow.has(clock.dow)
  if (fields.anyDom && fields.anyDow) return true
  if (fields.anyDom) return dowOk
  if (fields.anyDow) return domOk
  return domOk || dowOk
}

const MINUTE = 60 * 1000

/** The start of the minute `date` is in. */
export function floorMinute(date: Date): Date {
  return new Date(Math.floor(date.getTime() / MINUTE) * MINUTE)
}

/**
 * The LATEST minute in (`after`, `until`] the expression matches, or null.
 * Walks minute by minute from `until` back to `after` (exclusive), so a
 * window of a day costs at most 1440 checks.
 */
export function latestCronMatch(
  fields: CronFields,
  after: Date,
  until: Date,
  tz: string | null | undefined
): Date | null {
  const floorAfter = floorMinute(after).getTime()
  for (let at = floorMinute(until).getTime(); at > floorAfter; at -= MINUTE) {
    const date = new Date(at)
    if (cronMatches(fields, date, tz)) return date
  }
  return null
}
