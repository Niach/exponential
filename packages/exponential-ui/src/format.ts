// Round 2 (docs/round-2-contract.md §3): FORMATTING through a host-supplied
// Formatter. A renderer formats every number, currency, percent, date and
// relative time it shows (Table cells, NumberField, chart ticks, calendar
// names, the format* bind functions) through ONE Formatter per surface, in
// the surface's `locale` and `timeZone`. The TS renderers build it on Intl
// (`intlFormatter`); the Rust core defines a `Formatter` trait whose default
// is the ENGLISH fallback (`englishFormatter`, catalog/locale.json
// `englishFallback`); FFI hosts pass a foreign one. fixtures/format.json locks
// the fallback (= Intl en-US, ICU 74+; instants in the surface zone, given
// to the fallback as a UTC offset per instant), and the display of a bound non-string
// value in a text prop (`displayString`). Pure.

import localeJson from "../catalog/locale.json" with { type: "json" }

export type PluralCategory = `zero` | `one` | `two` | `few` | `many` | `other`
export type DateStyle = `short` | `medium` | `long` | `full`
export type RelativeUnit = `second` | `minute` | `hour` | `day` | `week` | `month` | `year`

export interface NumberOptions {
  /** Fixed fraction digits (min = max); absent = 0..3 for numbers, the
   *  currency's minor digits for currencies, 0 for percents. */
  decimals?: number
  /** Locale grouping separators (default true). */
  grouping?: boolean
}

export interface DateOptions {
  /** A Unicode TR35 pattern (`MMM d, yyyy`, `HH:mm`); wins over `style`. */
  format?: string
  /** A locale preset (default `medium`: `Oct 14, 2026`). */
  style?: DateStyle
  /** Append the short time (`2:30 PM`) to a preset. */
  time?: boolean
}

/** What a host supplies per surface (SurfaceSettings.locale + timeZone). */
export interface Formatter {
  readonly locale: string
  number(value: number, options?: NumberOptions): string
  currency(value: number, currency: string, options?: NumberOptions): string
  /** `value` is a ratio: 0.256 → `26%`. */
  percent(value: number, options?: { decimals?: number }): string
  /** `value` = `yyyy-mm-dd` (a calendar day, no zone), an ISO date-time (an
   *  instant; no offset = UTC) or epoch milliseconds; unreadable = ``. */
  date(value: unknown, options?: DateOptions): string
  /** `value` as for `date`, against `now` (epoch ms). */
  relativeTime(value: unknown, now: number): string
  plural(value: number): PluralCategory
}

const FALLBACK = localeJson.englishFallback
const MS = { second: 1000, minute: 60_000, hour: 3_600_000, day: 86_400_000 }

// ---------------------------------------------------------------------------
// Display of a non-string value in a text prop
// ---------------------------------------------------------------------------

/** What a `string`/`markdown` prop shows for a bound value: strings as they
 *  are, finite numbers as ECMAScript `Number.prototype.toString` (shortest
 *  round trip, `-0` → `0`, exponent form below 1e-6 and from 1e21), booleans
 *  `true`/`false`; null, undefined, non-finite numbers, objects and arrays
 *  show nothing (``). No locale formatting (use formatNumber for that). */
export function displayString(value: unknown): string {
  if (typeof value === `string`) return value
  if (typeof value === `number`) return Number.isFinite(value) ? String(value) : ``
  if (typeof value === `boolean`) return String(value)
  return ``
}

// ---------------------------------------------------------------------------
// Decimal arithmetic on the shortest round-trip string (ICU's halfExpand)
// ---------------------------------------------------------------------------

interface Decimal {
  int: string
  frac: string
}

/** |n| as its shortest round-trip decimal digits, split at the point. */
function decimalOf(abs: number): Decimal {
  const m = /^(\d+)(?:\.(\d+))?(?:e([+-]\d+))?$/.exec(String(abs))
  if (!m) return { int: `0`, frac: `` }
  const digits = m[1] + (m[2] ?? ``)
  let point = m[1].length + (m[3] ? Number(m[3]) : 0)
  let all = digits
  if (point <= 0) {
    all = `0`.repeat(1 - point) + all
    point = 1
  }
  if (point > all.length) all = all + `0`.repeat(point - all.length)
  const int = all.slice(0, point).replace(/^0+(?=\d)/, ``)
  return { int, frac: all.slice(point).replace(/0+$/, ``) }
}

/** Move the point `places` to the right (percent = 2). */
function shift(d: Decimal, places: number): Decimal {
  const frac = d.frac + `0`.repeat(Math.max(0, places - d.frac.length))
  const int = (d.int + frac.slice(0, places)).replace(/^0+(?=\d)/, ``)
  return { int, frac: frac.slice(places).replace(/0+$/, ``) }
}

/** Round to at most `max` fraction digits, half away from zero, then pad to
 *  `min`. */
function round(d: Decimal, min: number, max: number): Decimal {
  let { int, frac } = d
  if (frac.length > max) {
    const up = frac.charCodeAt(max) >= 53 // '5'
    let kept = int + frac.slice(0, max)
    if (up) {
      const chars = kept.split(``)
      let i = chars.length - 1
      for (; i >= 0; i--) {
        if (chars[i] === `9`) chars[i] = `0`
        else {
          chars[i] = String(Number(chars[i]) + 1)
          break
        }
      }
      kept = (i < 0 ? `1` : ``) + chars.join(``)
    }
    const intLen = kept.length - max
    int = kept.slice(0, intLen) || `0`
    frac = kept.slice(intLen).replace(/0+$/, ``)
  }
  if (frac.length < min) frac = frac + `0`.repeat(min - frac.length)
  return { int, frac }
}

function group(int: string, on: boolean): string {
  if (!on || int.length < 4) return int
  let out = ``
  for (let i = 0; i < int.length; i++) {
    if (i > 0 && (int.length - i) % 3 === 0) out += `,`
    out += int[i]
  }
  return out
}

function englishNumber(value: number, min: number, max: number, grouping: boolean): { sign: string; body: string } {
  const neg = value < 0 || Object.is(value, -0)
  if (Number.isNaN(value)) return { sign: ``, body: `NaN` }
  if (!Number.isFinite(value)) return { sign: neg ? `-` : ``, body: `∞` }
  const d = round(decimalOf(Math.abs(value)), min, max)
  return { sign: neg ? `-` : ``, body: `${group(d.int, grouping)}${d.frac ? `.${d.frac}` : ``}` }
}

const fixedDigits = (decimals: number | undefined): number | undefined =>
  typeof decimals === `number` && Number.isFinite(decimals) ? Math.max(0, Math.min(20, Math.trunc(decimals))) : undefined

/** The minor digits of a currency (ISO 4217; 2 when not listed). */
export function currencyDigits(code: string): number {
  return (FALLBACK.currencyDigits as Record<string, number>)[code.toUpperCase()] ?? 2
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

/** The instant a date value names: `yyyy-mm-dd` = that calendar day (UTC
 *  midnight, `dateOnly`), an ISO date-time (no offset = UTC), epoch ms. */
export function parseDateValue(value: unknown): { ms: number; dateOnly: boolean } | null {
  if (typeof value === `number`) return Number.isFinite(value) ? { ms: value, dateOnly: false } : null
  if (typeof value !== `string` || value.trim() === ``) return null
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (day) {
    const ms = Date.UTC(Number(day[1]), Number(day[2]) - 1, Number(day[3]))
    return Number.isFinite(ms) ? { ms, dateOnly: true } : null
  }
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})?$/.test(value)) return null
  const zoned = /(Z|[+-]\d{2}:\d{2})$/.test(value) ? value : `${value}Z`
  const ms = Date.parse(zoned)
  return Number.isFinite(ms) ? { ms, dateOnly: false } : null
}

interface DateFields {
  year: number
  month: number // 1..12
  day: number
  weekday: number // 0 = Sunday
  hour: number
  minute: number
  second: number
}

function utcFields(ms: number): DateFields {
  const d = new Date(ms)
  return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, day: d.getUTCDate(), weekday: d.getUTCDay(), hour: d.getUTCHours(), minute: d.getUTCMinutes(), second: d.getUTCSeconds() }
}

interface DateNames {
  months: readonly string[]
  monthsShort: readonly string[]
  weekdays: readonly string[]
  weekdaysShort: readonly string[]
  dayPeriods: readonly string[]
}

const pad = (n: number, width: number) => String(n).padStart(width, `0`)

/** A TR35 pattern over fields: `y yy yyyy M MM MMM MMMM d dd E EEE EEEE h hh
 *  H HH m mm s ss a`, `'quoted'` literals (`''` = an apostrophe); any other
 *  letter run is copied as written. Digits are ASCII. */
export function formatPattern(pattern: string, f: DateFields, names: DateNames): string {
  let out = ``
  let i = 0
  while (i < pattern.length) {
    const ch = pattern[i]
    if (ch === `'`) {
      if (pattern[i + 1] === `'`) {
        out += `'`
        i += 2
        continue
      }
      // A quoted literal; `''` inside it is an apostrophe too.
      i++
      while (i < pattern.length) {
        if (pattern[i] === `'`) {
          if (pattern[i + 1] === `'`) {
            out += `'`
            i += 2
            continue
          }
          i++
          break
        }
        out += pattern[i]
        i++
      }
      continue
    }
    if (!/[A-Za-z]/.test(ch)) {
      out += ch
      i++
      continue
    }
    let n = 1
    while (pattern[i + n] === ch) n++
    const run = pattern.slice(i, i + n)
    i += n
    const h12 = f.hour % 12 === 0 ? 12 : f.hour % 12
    switch (ch) {
      case `y`:
        out += n === 2 ? pad(f.year % 100, 2) : pad(f.year, n)
        break
      case `M`:
        out += n >= 4 ? names.months[f.month - 1] : n === 3 ? names.monthsShort[f.month - 1] : pad(f.month, n)
        break
      case `d`:
        out += pad(f.day, n)
        break
      case `E`:
        out += n >= 4 ? names.weekdays[f.weekday] : names.weekdaysShort[f.weekday]
        break
      case `h`:
        out += pad(h12, n)
        break
      case `H`:
        out += pad(f.hour, n)
        break
      case `m`:
        out += pad(f.minute, n)
        break
      case `s`:
        out += pad(f.second, n)
        break
      case `a`:
        out += names.dayPeriods[f.hour < 12 ? 0 : 1]
        break
      default:
        out += run
    }
  }
  return out
}

/** The en-US preset patterns (`style`), the time a preset appends and the
 *  joiner per style (locale.json `englishFallback`). */
export const ENGLISH_DATE_PATTERNS: Readonly<Record<DateStyle, string>> = FALLBACK.datePatterns
export const ENGLISH_TIME_PATTERN: string = FALLBACK.timePattern
export const ENGLISH_DATE_TIME_JOIN: Readonly<Record<DateStyle, string>> = FALLBACK.dateTimeJoin

/** The surface zone as the English fallback sees it: the UTC offset in
 *  minutes (east positive) in force at an instant (epoch ms). A host
 *  without an IANA database passes its platform's local offset; the
 *  fixtures pin a fixed one (`offsetMinutes`). */
export type ZoneOffset = (utcMs: number) => number

/** A fixed offset (minutes east of UTC); `fixedOffset(0)` = UTC. */
export const fixedOffset = (minutes: number): ZoneOffset => () => minutes

/** An IANA zone's offset through Intl (TS hosts; the Rust core takes its
 *  host's offset instead). */
export function intlZoneOffset(timeZone: string): ZoneOffset {
  const dtf = new Intl.DateTimeFormat(`en-US`, { timeZone, year: `numeric`, month: `numeric`, day: `numeric`, hour: `numeric`, minute: `numeric`, second: `numeric`, hourCycle: `h23` })
  return (ms) => {
    const at = Math.floor(ms / 1000) * 1000
    const p = Object.fromEntries(dtf.formatToParts(new Date(at)).map((x) => [x.type, x.value]))
    const local = Date.UTC(Number(p.year), Number(p.month) - 1, Number(p.day), Number(p.hour) % 24, Number(p.minute), Number(p.second))
    return Math.round((local - at) / 60_000)
  }
}

// ---------------------------------------------------------------------------
// Relative time
// ---------------------------------------------------------------------------

/** The unit a delta (value − now, ms) is shown in, and its count (truncated
 *  toward zero): under a minute seconds, under an hour minutes, under a day
 *  hours, under 7 days days, under 30 days weeks, under 365 days months (of
 *  30 days), else years (of 365 days). */
export function relativeTimeUnit(deltaMs: number): { value: number; unit: RelativeUnit } {
  const abs = Math.abs(deltaMs)
  const t = (unit: number) => Math.trunc(deltaMs / unit) || 0
  if (abs < MS.minute) return { value: t(MS.second), unit: `second` }
  if (abs < MS.hour) return { value: t(MS.minute), unit: `minute` }
  if (abs < MS.day) return { value: t(MS.hour), unit: `hour` }
  if (abs < 7 * MS.day) return { value: t(MS.day), unit: `day` }
  if (abs < 30 * MS.day) return { value: t(7 * MS.day), unit: `week` }
  if (abs < 365 * MS.day) return { value: t(30 * MS.day), unit: `month` }
  return { value: t(365 * MS.day), unit: `year` }
}

// ---------------------------------------------------------------------------
// The English fallback
// ---------------------------------------------------------------------------

const ENGLISH_NAMES: DateNames = FALLBACK

/** The Formatter a renderer uses without a platform ICU (the Rust core's
 *  default): Intl en-US, decided by data (catalog/locale.json
 *  `englishFallback`). Instants show in the surface zone `zone` (default
 *  UTC); a `yyyy-mm-dd` value is a calendar day and never shifts. */
export function englishFormatter(zone: ZoneOffset = fixedOffset(0)): Formatter {
  return {
    locale: `en-US`,
    number(value, options = {}) {
      const fixed = fixedDigits(options.decimals)
      const { sign, body } = englishNumber(value, fixed ?? 0, fixed ?? 3, options.grouping !== false)
      return `${sign}${body}`
    },
    currency(value, currency, options = {}) {
      const code = currency.toUpperCase()
      const digits = fixedDigits(options.decimals) ?? currencyDigits(code)
      const { sign, body } = englishNumber(value, digits, digits, options.grouping !== false)
      const prefix = (FALLBACK.currencyPrefix as Record<string, string>)[code] ?? FALLBACK.unknownCurrency.replace(`{code}`, code)
      return `${sign}${prefix}${body}`
    },
    percent(value, options = {}) {
      const digits = fixedDigits(options.decimals) ?? 0
      if (!Number.isFinite(value)) {
        const { sign, body } = englishNumber(value, 0, 0, true)
        return `${sign}${body}%`
      }
      const neg = value < 0 || Object.is(value, -0)
      const d = round(shift(decimalOf(Math.abs(value)), 2), digits, digits)
      return `${neg ? `-` : ``}${group(d.int, true)}${d.frac ? `.${d.frac}` : ``}%`
    },
    date(value, options = {}) {
      const parsed = parseDateValue(value)
      if (!parsed) return ``
      const f = utcFields(parsed.dateOnly ? parsed.ms : parsed.ms + zone(parsed.ms) * 60_000)
      if (options.format) return formatPattern(options.format, f, ENGLISH_NAMES)
      const style = options.style ?? `medium`
      const date = formatPattern(ENGLISH_DATE_PATTERNS[style], f, ENGLISH_NAMES)
      if (!options.time) return date
      const time = formatPattern(ENGLISH_TIME_PATTERN, f, ENGLISH_NAMES)
      return ENGLISH_DATE_TIME_JOIN[style].replace(`{date}`, date).replace(`{time}`, time)
    },
    relativeTime(value, now) {
      const parsed = parseDateValue(value)
      if (!parsed) return ``
      const { value: n, unit } = relativeTimeUnit(parsed.ms - now)
      const auto = (FALLBACK.relative.auto as Record<string, Record<string, string>>)[unit]?.[String(n)]
      if (auto) return auto
      const [one, many] = (FALLBACK.relative.units as Record<string, string[]>)[unit]
      const count = englishNumber(Math.abs(n), 0, 0, true).body
      const unitName = Math.abs(n) === 1 ? one : many
      const template = n < 0 ? FALLBACK.relative.past : FALLBACK.relative.future
      return template.replace(`{value}`, count).replace(`{unit}`, unitName)
    },
    plural(value) {
      return Math.abs(value) === 1 ? `one` : `other`
    },
  }
}

// ---------------------------------------------------------------------------
// The Intl formatter (TS renderers)
// ---------------------------------------------------------------------------

/** A Formatter on the platform's Intl in `locale` (BCP 47) and `timeZone`
 *  (IANA; default UTC, as the fixtures pin it). Patterns use the locale's
 *  month/weekday/day-period NAMES with ASCII digits. */
export function intlFormatter(locale: string, timeZone = `UTC`): Formatter {
  const nf = (o: Intl.NumberFormatOptions) => new Intl.NumberFormat(locale, o)
  let names: DateNames | null = null
  const dateNames = (): DateNames => {
    if (names) return names
    const at = (o: Intl.DateTimeFormatOptions, ms: number) => new Intl.DateTimeFormat(locale, { ...o, timeZone: `UTC` }).format(new Date(ms))
    const months = Array.from({ length: 12 }, (_, i) => Date.UTC(2026, i, 1))
    const days = Array.from({ length: 7 }, (_, i) => Date.UTC(2026, 0, 4 + i)) // 2026-01-04 is a Sunday
    const period = (h: number) => new Intl.DateTimeFormat(locale, { hour: `numeric`, hour12: true, timeZone: `UTC` }).formatToParts(new Date(Date.UTC(2026, 0, 1, h))).find((p) => p.type === `dayPeriod`)?.value ?? (h < 12 ? `AM` : `PM`)
    names = {
      months: months.map((ms) => at({ month: `long` }, ms)),
      monthsShort: months.map((ms) => at({ month: `short` }, ms)),
      weekdays: days.map((ms) => at({ weekday: `long` }, ms)),
      weekdaysShort: days.map((ms) => at({ weekday: `short` }, ms)),
      dayPeriods: [period(9), period(15)],
    }
    return names
  }
  const zonedFields = (ms: number): DateFields => {
    const parts = new Intl.DateTimeFormat(`en-US`, { timeZone, year: `numeric`, month: `numeric`, day: `numeric`, hour: `numeric`, minute: `numeric`, second: `numeric`, weekday: `short`, hourCycle: `h23` }).formatToParts(new Date(ms))
    const get = (t: string) => parts.find((p) => p.type === t)?.value ?? `0`
    return { year: Number(get(`year`)), month: Number(get(`month`)), day: Number(get(`day`)), weekday: [`Sun`, `Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat`].indexOf(get(`weekday`)), hour: Number(get(`hour`)) % 24, minute: Number(get(`minute`)), second: Number(get(`second`)) }
  }
  return {
    locale,
    number(value, options = {}) {
      const fixed = fixedDigits(options.decimals)
      return nf({ minimumFractionDigits: fixed ?? 0, maximumFractionDigits: fixed ?? 3, useGrouping: options.grouping !== false }).format(value)
    },
    currency(value, currency, options = {}) {
      const fixed = fixedDigits(options.decimals)
      return nf({ style: `currency`, currency, ...(fixed !== undefined ? { minimumFractionDigits: fixed, maximumFractionDigits: fixed } : {}), useGrouping: options.grouping !== false }).format(value)
    },
    percent(value, options = {}) {
      const fixed = fixedDigits(options.decimals) ?? 0
      return nf({ style: `percent`, minimumFractionDigits: fixed, maximumFractionDigits: fixed }).format(value)
    },
    date(value, options = {}) {
      const parsed = parseDateValue(value)
      if (!parsed) return ``
      const zone = parsed.dateOnly ? `UTC` : timeZone
      if (options.format) return formatPattern(options.format, parsed.dateOnly ? utcFields(parsed.ms) : zonedFields(parsed.ms), dateNames())
      return new Intl.DateTimeFormat(locale, { dateStyle: options.style ?? `medium`, ...(options.time ? { timeStyle: `short` } : {}), timeZone: zone }).format(new Date(parsed.ms))
    },
    relativeTime(value, now) {
      const parsed = parseDateValue(value)
      if (!parsed) return ``
      const { value: n, unit } = relativeTimeUnit(parsed.ms - now)
      return new Intl.RelativeTimeFormat(locale, { numeric: `auto` }).format(n, unit)
    },
    plural(value) {
      return new Intl.PluralRules(locale).select(value) as PluralCategory
    },
  }
}

// ---------------------------------------------------------------------------
// The format functions of the bind table
// ---------------------------------------------------------------------------

/** The bind-time format functions (A2UI basic `formatNumber`,
 *  `formatCurrency`, `formatDate`, `pluralize` + the core `formatPercent`,
 *  `formatRelativeTime`), all through ONE Formatter. */
export const FORMAT_FUNCTION_NAMES = [`formatNumber`, `formatCurrency`, `formatPercent`, `formatDate`, `formatRelativeTime`, `pluralize`] as const

/** A numeric string: optional ASCII whitespace around an optional sign,
 *  decimal digits with an optional fraction and exponent (`' 12 '`, `1e3`,
 *  `-.5`). Hex, `Infinity`, separators and anything else are not numbers. */
const NUMERIC = /^[ \t\n\r]*[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[ \t\n\r]*$/
const toNum = (v: unknown): number => (typeof v === `number` ? v : typeof v === `string` && NUMERIC.test(v) ? Number(v) : NaN)
const optNum = (v: unknown): number | undefined => (v === undefined || v === null ? undefined : toNum(v))

/** The format functions over a Formatter; `now` = the clock
 *  `formatRelativeTime` reads when its call has no `now` (a renderer
 *  re-binds such a node at least once a minute). A value that is not a
 *  number, a numeric string (`NUMERIC`) or a date formats to ``. */
export function formatFunctions(formatter: Formatter, now: () => number = () => Date.now()): Record<(typeof FORMAT_FUNCTION_NAMES)[number], (args: Record<string, unknown>) => unknown> {
  const numberOr = (value: unknown, f: (n: number) => string) => {
    const n = toNum(value)
    return Number.isFinite(n) ? f(n) : ``
  }
  return {
    formatNumber: ({ value, decimals, grouping }) => numberOr(value, (n) => formatter.number(n, { decimals: optNum(decimals), grouping: grouping === undefined ? undefined : grouping !== false })),
    formatCurrency: ({ value, currency, decimals, grouping }) => numberOr(value, (n) => (typeof currency === `string` && /^[A-Za-z]{3}$/.test(currency) ? formatter.currency(n, currency, { decimals: optNum(decimals), grouping: grouping === undefined ? undefined : grouping !== false }) : ``)),
    formatPercent: ({ value, decimals }) => numberOr(value, (n) => formatter.percent(n, { decimals: optNum(decimals) })),
    formatDate: ({ value, format, style, time }) =>
      formatter.date(value, { ...(typeof format === `string` && format ? { format } : {}), ...(style === `short` || style === `medium` || style === `long` || style === `full` ? { style } : {}), ...(time === true ? { time: true } : {}) }),
    formatRelativeTime: ({ value, now: at }) => formatter.relativeTime(value, typeof at === `number` ? at : (parseDateValue(at)?.ms ?? now())),
    pluralize: (args) => {
      const n = toNum(args.value)
      if (!Number.isFinite(n)) return args.other
      if (n === 0 && args.zero !== undefined) return args.zero
      const category = formatter.plural(n)
      return args[category] ?? args.other
    },
  }
}

/** The fallback's format functions (the reference bind table's). */
export const ENGLISH_FORMAT_FUNCTIONS = formatFunctions(englishFormatter())
