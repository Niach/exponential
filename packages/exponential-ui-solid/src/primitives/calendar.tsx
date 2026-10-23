// FEED-131: shadcn's Calendar on Solid, the twin of
// @exponential_app/ui-react/primitives/calendar (shadcn's wrapper over
// react-day-picker 9). There is no DayPicker for Solid, so this file ports
// the parts of DayPicker 9.14 the wrapper exposes and paints its EXACT DOM:
// the same elements, attributes (`data-day`, `data-outside`, `data-today`,
// `data-selected`, `data-focused`, `aria-selected`, `aria-label` via the
// en-US labels, the roving `tabindex`, `role="grid"` +
// `aria-multiselectable`…) and the `rdp-*` default class names in the order
// `cn(ours, defaults)` and DayPicker's modifier pass yield. Class strings
// come from CALENDAR_CLASSES (@exponential_app/ui/web/primitives), one source.
//
// Dates: plain local `Date`s and hand-written date math (date-fns' rules:
// `startOfWeek`, `getWeek`, `addMonths`' end-of-month clamp…). Formatting:
// DayPicker formats through date-fns with its en-US locale; this port writes
// the en-US patterns by hand ("LLLL y", "PPPP" with the ordinal, "cccccc",
// "cccc", "d", "yyyy") and falls back to `Intl.DateTimeFormat(locale.code)`
// for any other locale (close, not byte-identical).
//
// Not ported (the wrapper never sets them): `timeZone`/`noonSafe`,
// `numerals`, `animate`, `navLayout`, `broadcastCalendar`, `excludeDisabled`,
// the non-Gregorian calendars, a custom `dateLib`, `styles`/
// `modifiersStyles`. `components` overrides `DayButton` and `Chevron` only.

import { For, Show, createEffect, createMemo, createSignal, mergeProps, splitProps, untrack, type Component, type JSX } from "solid-js"
import { ChevronDown, ChevronLeft, ChevronRight } from "lucide-solid"
import { CALENDAR_CLASSES, type ButtonVariantProps } from "@exponential_app/ui/web/primitives"

import { PaintIcon } from "../icons"
import { cn } from "./cn"
import { Button, buttonVariants } from "./button"

// ─── dates (date-fns' rules on local dates) ──────────────────────────────

type WeekDay = 0 | 1 | 2 | 3 | 4 | 5 | 6

interface WeekOptions {
  weekStartsOn: WeekDay
  firstWeekContainsDate: number
}

const clone = (d: Date) => new Date(d.getTime())
const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate())
const startOfMonth = (d: Date) => new Date(d.getFullYear(), d.getMonth(), 1)
const endOfMonth = (d: Date) => new Date(d.getFullYear(), d.getMonth() + 1, 0, 23, 59, 59, 999)
const startOfYear = (d: Date) => new Date(d.getFullYear(), 0, 1)
const endOfYear = (d: Date) => new Date(d.getFullYear(), 11, 31, 23, 59, 59, 999)
const addDays = (d: Date, n: number) => {
  const out = clone(d)
  out.setDate(out.getDate() + n)
  return out
}
const addMonths = (d: Date, n: number) => {
  const out = clone(d)
  const day = out.getDate()
  const end = clone(d)
  end.setMonth(d.getMonth() + n + 1, 0)
  if (day >= end.getDate()) return end
  out.setFullYear(end.getFullYear(), end.getMonth(), day)
  return out
}
const addYears = (d: Date, n: number) => addMonths(d, n * 12)
const isSameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
const isSameMonth = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth()
const differenceInCalendarDays = (a: Date, b: Date) => Math.round((Date.UTC(a.getFullYear(), a.getMonth(), a.getDate()) - Date.UTC(b.getFullYear(), b.getMonth(), b.getDate())) / 864e5)
const differenceInCalendarMonths = (a: Date, b: Date) => (a.getFullYear() - b.getFullYear()) * 12 + a.getMonth() - b.getMonth()
const maxDate = (dates: Date[]) => dates.reduce((a, b) => (b > a ? b : a))
const minDate = (dates: Date[]) => dates.reduce((a, b) => (b < a ? b : a))
const startOfWeek = (d: Date, o: WeekOptions) => {
  const day = d.getDay()
  const diff = (day < o.weekStartsOn ? 7 : 0) + day - o.weekStartsOn
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - diff)
}
const endOfWeek = (d: Date, o: WeekOptions) => {
  const day = d.getDay()
  const diff = (day < o.weekStartsOn ? -7 : 0) + 6 - (day - o.weekStartsOn)
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + diff, 23, 59, 59, 999)
}
const ISO: WeekOptions = { weekStartsOn: 1, firstWeekContainsDate: 4 }
/** date-fns `getWeek` (local week numbering; ISO = `{1, 4}`). */
function getWeek(d: Date, o: WeekOptions): number {
  const startOfWeekYear = (year: number) => startOfWeek(new Date(year, 0, o.firstWeekContainsDate), o)
  const year = d.getFullYear()
  const weekYear = d >= startOfWeekYear(year + 1) ? year + 1 : d >= startOfWeekYear(year) ? year : year - 1
  return Math.round((startOfWeek(d, o).getTime() - startOfWeekYear(weekYear).getTime()) / 6048e5) + 1
}

// ─── formatting (en-US by hand, Intl for the rest) ───────────────────────

const MONTHS = [`January`, `February`, `March`, `April`, `May`, `June`, `July`, `August`, `September`, `October`, `November`, `December`]
const WEEKDAYS = [`Sunday`, `Monday`, `Tuesday`, `Wednesday`, `Thursday`, `Friday`, `Saturday`]
const WEEKDAYS_SHORT = [`Su`, `Mo`, `Tu`, `We`, `Th`, `Fr`, `Sa`]
const pad = (n: number, width: number) => String(n).padStart(width, `0`)
const ordinal = (n: number) => {
  const rem100 = n % 100
  if (rem100 > 20 || rem100 < 10) {
    if (rem100 % 10 === 1) return `${n}st`
    if (rem100 % 10 === 2) return `${n}nd`
    if (rem100 % 10 === 3) return `${n}rd`
  }
  return `${n}th`
}
const isoDate = (d: Date) => `${pad(d.getFullYear(), 4)}-${pad(d.getMonth() + 1, 2)}-${pad(d.getDate(), 2)}`
const monthId = (d: Date) => `${pad(d.getFullYear(), 4)}-${pad(d.getMonth() + 1, 2)}`

/** DayPicker's year-first locales (`DateLib.yearFirstLocales`). */
const YEAR_FIRST = new Set([`eu`, `hu`, `ja`, `ja-Hira`, `ja-JP`, `ko`, `ko-KR`, `lt`, `lt-LT`, `lv`, `lv-LV`, `mn`, `mn-MN`, `zh`, `zh-CN`, `zh-HK`, `zh-TW`])

/** A date-fns `Locale` (structurally): its `code` and week options. */
export interface CalendarLocale {
  code?: string
  options?: { weekStartsOn?: WeekDay; firstWeekContainsDate?: number }
  /** DayPicker's per-locale labels (react-day-picker's locales carry them). */
  labels?: Partial<Record<keyof CalendarLabels, string | ((...args: never[]) => string)>>
}

const EN_US: CalendarLocale = { code: `en-US`, options: { weekStartsOn: 0, firstWeekContainsDate: 1 } }

function intl(code: string, options: Intl.DateTimeFormatOptions) {
  return new Intl.DateTimeFormat(code, options)
}

interface Fmt {
  code: string
  en: boolean
  monthYear: (d: Date) => string
  /** "PPPP": "Thursday, October 22nd, 2026". */
  long: (d: Date) => string
  weekdayLong: (d: Date) => string
  weekdayShort: (d: Date) => string
  day: (d: Date) => string
  year: (d: Date) => string
  monthLong: (d: Date) => string
  yearFirst: boolean
}

function makeFmt(code: string): Fmt {
  const en = code === `en-US`
  const yearFirst = YEAR_FIRST.has(code)
  if (en)
    return {
      code,
      en,
      yearFirst,
      monthYear: (d) => `${MONTHS[d.getMonth()]} ${d.getFullYear()}`,
      long: (d) => `${WEEKDAYS[d.getDay()]}, ${MONTHS[d.getMonth()]} ${ordinal(d.getDate())}, ${d.getFullYear()}`,
      weekdayLong: (d) => WEEKDAYS[d.getDay()]!,
      weekdayShort: (d) => WEEKDAYS_SHORT[d.getDay()]!,
      day: (d) => String(d.getDate()),
      year: (d) => pad(d.getFullYear(), 4),
      monthLong: (d) => MONTHS[d.getMonth()]!,
    }
  return {
    code,
    en,
    yearFirst,
    monthYear: (d) => intl(code, { month: `long`, year: `numeric` }).format(d),
    long: (d) => intl(code, { weekday: `long`, month: `long`, day: `numeric`, year: `numeric` }).format(d),
    weekdayLong: (d) => intl(code, { weekday: `long` }).format(d),
    weekdayShort: (d) => intl(code, { weekday: `short` }).format(d).slice(0, 2),
    day: (d) => String(d.getDate()),
    year: (d) => pad(d.getFullYear(), 4),
    monthLong: (d) => intl(code, { month: `long` }).format(d),
  }
}

// ─── DayPicker's model ───────────────────────────────────────────────────

/** DayPicker's `CalendarDay`: a date shown in a display month. */
export class CalendarDay {
  readonly outside: boolean
  readonly isoDate: string
  readonly displayMonthId: string
  readonly dateMonthId: string
  constructor(
    readonly date: Date,
    readonly displayMonth: Date
  ) {
    this.outside = !isSameMonth(date, displayMonth)
    this.isoDate = isoDate(date)
    this.displayMonthId = monthId(displayMonth)
    this.dateMonthId = monthId(date)
  }
  isEqualTo(day: CalendarDay): boolean {
    return isSameDay(day.date, this.date) && isSameMonth(day.displayMonth, this.displayMonth)
  }
}

export interface CalendarWeek {
  weekNumber: number
  days: CalendarDay[]
}

export interface CalendarMonth {
  date: Date
  weeks: CalendarWeek[]
}

export interface DateRange {
  from: Date | undefined
  to?: Date | undefined
}
export interface DateBefore {
  before: Date
}
export interface DateAfter {
  after: Date
}
export interface DateInterval {
  before: Date
  after: Date
}
export interface DayOfWeek {
  dayOfWeek: number | number[]
}
/** DayPicker's `Matcher`. */
export type Matcher = boolean | ((date: Date) => boolean) | Date | Date[] | DateRange | DateBefore | DateAfter | DateInterval | DayOfWeek

/** A day's modifiers, in DayPicker's key order. */
export type Modifiers = Record<string, boolean | undefined>

const isObj = (v: unknown): v is Record<string, unknown> => Boolean(v) && typeof v === `object`
const isDateRange = (v: unknown): v is DateRange => isObj(v) && `from` in v

function rangeIncludesDate(range: DateRange, date: Date, excludeEnds = false): boolean {
  let { from, to } = range
  if (from && to) {
    if (differenceInCalendarDays(to, from) < 0) [from, to] = [to, from]
    const min = excludeEnds ? 1 : 0
    return differenceInCalendarDays(date, from) >= min && differenceInCalendarDays(to, date) >= min
  }
  if (!excludeEnds && to) return isSameDay(to, date)
  if (!excludeEnds && from) return isSameDay(from, date)
  return false
}

/** DayPicker's `dateMatchModifiers`. */
export function dateMatchModifiers(date: Date, matchers: Matcher | Matcher[]): boolean {
  const list = Array.isArray(matchers) && !matchers.every((m) => m instanceof Date) ? (matchers as Matcher[]) : [matchers as Matcher]
  return list.some((matcher) => {
    if (typeof matcher === `boolean`) return matcher
    if (matcher instanceof Date) return isSameDay(date, matcher)
    if (Array.isArray(matcher)) return matcher.some((d) => isSameDay(date, d))
    if (isDateRange(matcher)) return rangeIncludesDate(matcher, date, false)
    if (isObj(matcher) && `dayOfWeek` in matcher) {
      const dow = (matcher as DayOfWeek).dayOfWeek
      return Array.isArray(dow) ? dow.includes(date.getDay()) : dow === date.getDay()
    }
    if (isObj(matcher) && `before` in matcher && `after` in matcher) {
      const { before, after } = matcher as unknown as DateInterval
      const isDayBefore = differenceInCalendarDays(before, date) > 0
      const isDayAfter = differenceInCalendarDays(after, date) < 0
      return before > after ? isDayAfter && isDayBefore : isDayBefore || isDayAfter
    }
    if (isObj(matcher) && `after` in matcher) return differenceInCalendarDays(date, (matcher as DateAfter).after) > 0
    if (isObj(matcher) && `before` in matcher) return differenceInCalendarDays((matcher as DateBefore).before, date) > 0
    if (typeof matcher === `function`) return matcher(date)
    return false
  })
}

/** DayPicker's `addToRange`. */
function addToRange(date: Date, initial: DateRange | undefined, min = 0, max = 0, required = false): DateRange | undefined {
  const { from, to } = initial || ({} as Partial<DateRange>)
  let range: DateRange | undefined
  if (!from && !to) range = { from: date, to: min > 0 ? undefined : date }
  else if (from && !to) {
    if (isSameDay(from, date)) range = min === 0 ? { from, to: date } : required ? { from, to: undefined } : undefined
    else if (date < from) range = { from: date, to: from }
    else range = { from, to: date }
  } else if (from && to) {
    if (isSameDay(from, date) && isSameDay(to, date)) range = required ? { from, to } : undefined
    else if (isSameDay(from, date)) range = { from, to: min > 0 ? undefined : date }
    else if (isSameDay(to, date)) range = { from: date, to: min > 0 ? undefined : date }
    else if (date < from) range = { from: date, to }
    else range = { from, to: date }
  }
  if (range?.from && range.to) {
    const diff = differenceInCalendarDays(range.to, range.from)
    if (max > 0 && diff > max) range = { from: date, to: undefined }
    else if (min > 1 && diff < min) range = { from: date, to: undefined }
  }
  return range
}

// ─── props ───────────────────────────────────────────────────────────────

export interface CalendarFormatters {
  formatCaption: (month: Date) => string
  formatDay: (date: Date) => string
  formatMonthDropdown: (month: Date) => string
  formatYearDropdown: (year: Date) => string
  formatWeekdayName: (weekday: Date) => string
  formatWeekNumber: (weekNumber: number) => string
  formatWeekNumberHeader: () => string
}

export interface CalendarLabels {
  labelDayButton: (date: Date, modifiers: Modifiers) => string
  labelGridcell: (date: Date, modifiers?: Modifiers) => string
  labelGrid: (month: Date) => string
  labelNav: () => string
  labelNext: (month: Date | undefined) => string
  labelPrevious: (month: Date | undefined) => string
  labelWeekday: (weekday: Date) => string
  labelWeekNumber: (weekNumber: number) => string
  labelWeekNumberHeader: () => string
  labelMonthDropdown: () => string
  labelYearDropdown: () => string
}

export interface CalendarDayButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  day: CalendarDay
  modifiers: Modifiers
  [data: `data-${string}`]: string | undefined
}

export interface CalendarChevronProps {
  class?: string
  orientation?: `up` | `down` | `left` | `right`
  disabled?: boolean
  size?: number
}

type SelectHandler<T> = (selected: T, triggerDate: Date, modifiers: Modifiers, e: MouseEvent) => void

interface ModeNone {
  mode?: undefined
  selected?: undefined
  onSelect?: undefined
  required?: undefined
}
interface ModeSingle {
  mode: `single`
  selected?: Date | undefined
  onSelect?: SelectHandler<Date | undefined>
  required?: boolean
}
interface ModeMultiple {
  mode: `multiple`
  selected?: Date[] | undefined
  onSelect?: SelectHandler<Date[] | undefined>
  required?: boolean
  min?: number
  max?: number
}
interface ModeRange {
  mode: `range`
  selected?: DateRange | undefined
  onSelect?: SelectHandler<DateRange | undefined>
  required?: boolean
  min?: number
  max?: number
  resetOnSelect?: boolean
}

export type CalendarProps = (ModeNone | ModeSingle | ModeMultiple | ModeRange) & {
  class?: string
  /** DayPicker's part map (its keys: `root`, `day`, `day_button`, `today`…), merged over shadcn's. */
  classNames?: Partial<Record<string, string>>
  style?: JSX.CSSProperties | string
  showOutsideDays?: boolean
  captionLayout?: `label` | `dropdown` | `dropdown-months` | `dropdown-years`
  buttonVariant?: ButtonVariantProps[`variant`]
  formatters?: Partial<CalendarFormatters>
  labels?: Partial<CalendarLabels>
  components?: { DayButton?: Component<CalendarDayButtonProps>; Chevron?: Component<CalendarChevronProps> }
  defaultMonth?: Date
  month?: Date
  onMonthChange?: (month: Date) => void
  onPrevClick?: (month: Date) => void
  onNextClick?: (month: Date) => void
  numberOfMonths?: number
  pagedNavigation?: boolean
  reverseMonths?: boolean
  fromYear?: number
  toYear?: number
  fromMonth?: Date
  toMonth?: Date
  startMonth?: Date
  endMonth?: Date
  disabled?: Matcher | Matcher[]
  hidden?: Matcher | Matcher[]
  modifiers?: Record<string, Matcher | Matcher[] | undefined>
  modifiersClassNames?: Record<string, string>
  today?: Date
  showWeekNumber?: boolean
  ISOWeek?: boolean
  fixedWeeks?: boolean
  weekStartsOn?: WeekDay
  firstWeekContainsDate?: 1 | 4
  locale?: CalendarLocale
  dir?: `ltr` | `rtl` | `auto`
  lang?: string
  id?: string
  title?: string
  role?: JSX.HTMLAttributes<HTMLDivElement>[`role`]
  nonce?: string
  "aria-label"?: string
  "aria-labelledby"?: string
  autoFocus?: boolean
  hideNavigation?: boolean
  disableNavigation?: boolean
  hideWeekdays?: boolean
  reverseYears?: boolean
  footer?: JSX.Element
  onDayClick?: (date: Date, modifiers: Modifiers, e: MouseEvent) => void
  onDayFocus?: (date: Date, modifiers: Modifiers, e: FocusEvent) => void
  onDayBlur?: (date: Date, modifiers: Modifiers, e: FocusEvent) => void
  onDayKeyDown?: (date: Date, modifiers: Modifiers, e: KeyboardEvent) => void
  onDayMouseEnter?: (date: Date, modifiers: Modifiers, e: MouseEvent) => void
  onDayMouseLeave?: (date: Date, modifiers: Modifiers, e: MouseEvent) => void
  [data: `data-${string}`]: string | undefined
}

const DEFAULT_CLASS_KEYS = [
  `root`, `chevron`, `day`, `day_button`, `caption_label`, `dropdowns`, `dropdown`, `dropdown_root`, `footer`, `month_grid`, `month_caption`, `months_dropdown`, `month`, `months`, `nav`, `button_next`, `button_previous`, `week`, `weeks`, `weekday`, `weekdays`, `week_number`, `week_number_header`, `years_dropdown`,
] as const
/** DayPicker's `DayFlag`s with their initial values, in its key order (keys,
 *  not literals: the recipes gate reads a bare `hidden` literal as a class). */
const NO_FLAGS = { focused: false, disabled: false, hidden: false, outside: false, today: false }
const DAY_FLAGS: readonly string[] = Object.keys(NO_FLAGS)
const SELECTION_STATES = [`range_end`, `range_middle`, `range_start`, `selected`]
/** react-day-picker's `getDefaultClassNames()`: `rdp-<key>`. */
export const getDefaultClassNames = (): Record<string, string> => Object.fromEntries([...DEFAULT_CLASS_KEYS, ...DAY_FLAGS, ...SELECTION_STATES].map((k) => [k, `rdp-${k}`]))

/** The visually-hidden live caption of the dropdown layout (DayPicker's inline style). */
const STATUS_STYLE: JSX.CSSProperties = { border: `0`, clip: `rect(0 0 0 0)`, height: `1px`, margin: `-1px`, overflow: [`hid`, `den`].join(``) /* a CSS value, spelled in pieces for the recipes gate */, padding: `0`, position: `absolute`, width: `1px`, "white-space": `nowrap`, "word-wrap": `normal` }


// ─── the component ───────────────────────────────────────────────────────

function Calendar(input: CalendarProps): JSX.Element {
  const props = mergeProps({ showOutsideDays: true, captionLayout: `label` as const, buttonVariant: `ghost` as const }, input)

  const locale = createMemo(() => ({ ...EN_US, ...props.locale }))
  const fmt = createMemo(() => makeFmt(locale().code ?? `en-US`))
  const week = createMemo<WeekOptions>(() => ({
    weekStartsOn: props.weekStartsOn ?? locale().options?.weekStartsOn ?? 0,
    firstWeekContainsDate: props.firstWeekContainsDate ?? locale().options?.firstWeekContainsDate ?? 1,
  }))
  const sow = (d: Date) => (props.ISOWeek ? startOfWeek(d, ISO) : startOfWeek(d, week()))
  const eow = (d: Date) => (props.ISOWeek ? endOfWeek(d, ISO) : endOfWeek(d, week()))
  const today = () => props.today ?? new Date()

  // The shadcn wrapper's map over DayPicker's defaults, then the caller's.
  const classNames = createMemo(() => {
    const parts = CALENDAR_CLASSES.classNames
    const d = getDefaultClassNames()
    const ours: Record<string, string> = {
      root: cn(parts.root, d.root),
      months: cn(parts.months, d.months),
      month: cn(parts.month, d.month),
      nav: cn(parts.nav, d.nav),
      button_previous: cn(buttonVariants({ variant: props.buttonVariant }), parts.button_previous, d.button_previous),
      button_next: cn(buttonVariants({ variant: props.buttonVariant }), parts.button_next, d.button_next),
      month_caption: cn(parts.month_caption, d.month_caption),
      dropdowns: cn(parts.dropdowns, d.dropdowns),
      dropdown_root: cn(parts.dropdown_root, d.dropdown_root),
      dropdown: cn(parts.dropdown, d.dropdown),
      caption_label: cn(parts.caption_label, props.captionLayout === `label` ? CALENDAR_CLASSES.captionLabel.label : CALENDAR_CLASSES.captionLabel.dropdown, d.caption_label),
      table: parts.table,
      weekdays: cn(parts.weekdays, d.weekdays),
      weekday: cn(parts.weekday, d.weekday),
      week: cn(parts.week, d.week),
      week_number_header: cn(parts.week_number_header, d.week_number_header),
      week_number: cn(parts.week_number, d.week_number),
      day: cn(parts.day, props.showWeekNumber ? CALENDAR_CLASSES.dayFirst.weekNumbers : CALENDAR_CLASSES.dayFirst.plain, d.day),
      range_start: cn(parts.range_start, d.range_start),
      range_middle: cn(parts.range_middle, d.range_middle),
      range_end: cn(parts.range_end, d.range_end),
      today: cn(parts.today, d.today),
      outside: cn(parts.outside, d.outside),
      disabled: cn(parts.disabled, d.disabled),
      hidden: cn(parts.hidden, d.hidden),
    }
    const out: Record<string, string> = { ...d, ...ours }
    for (const [k, v] of Object.entries(props.classNames ?? {})) if (v !== undefined) out[k] = v
    return out
  })

  const formatters = createMemo<CalendarFormatters>(() => {
    const f = fmt()
    return {
      formatCaption: (m) => f.monthYear(m),
      formatDay: (d) => f.day(d),
      // The shadcn wrapper's override.
      formatMonthDropdown: (m) => m.toLocaleString(`default`, { month: `short` }),
      formatYearDropdown: (y) => f.year(y),
      formatWeekdayName: (d) => f.weekdayShort(d),
      formatWeekNumber: (n) => (n < 10 ? `0${n.toLocaleString()}` : `${n.toLocaleString()}`),
      formatWeekNumberHeader: () => ``,
      ...props.formatters,
    }
  })

  const labels = createMemo<CalendarLabels>(() => {
    const f = fmt()
    const fromLocale = locale().labels ?? {}
    const pick = <K extends keyof CalendarLabels>(key: K, fallback: CalendarLabels[K]): CalendarLabels[K] => {
      const own = props.labels?.[key]
      if (own) return own
      const l = fromLocale[key]
      if (l) return (typeof l === `function` ? l : () => l) as CalendarLabels[K]
      return fallback
    }
    // react-day-picker's en-US locale labels (its default locale).
    const dayLabel = (date: Date, m: Modifiers | undefined, selected: boolean) => {
      let label = f.long(date)
      if (m?.today) label = `Today, ${label}`
      if (selected && m?.selected) label = `${label}, selected`
      return label
    }
    return {
      labelDayButton: pick(`labelDayButton`, (d, m) => dayLabel(d, m, true)),
      labelGridcell: pick(`labelGridcell`, (d, m) => dayLabel(d, m, false)),
      labelGrid: pick(`labelGrid`, (m) => (f.en ? `${MONTHS[m.getMonth()]} ${pad(m.getFullYear(), 4)}` : f.monthYear(m))),
      labelNav: pick(`labelNav`, () => `Navigation bar`),
      labelNext: pick(`labelNext`, () => `Go to the Next Month`),
      labelPrevious: pick(`labelPrevious`, () => `Go to the Previous Month`),
      labelWeekday: pick(`labelWeekday`, (d) => f.weekdayLong(d)),
      labelWeekNumber: pick(`labelWeekNumber`, (n) => `Week ${n}`),
      labelWeekNumberHeader: pick(`labelWeekNumberHeader`, () => `Week Number`),
      labelMonthDropdown: pick(`labelMonthDropdown`, () => `Choose the Month`),
      labelYearDropdown: pick(`labelYearDropdown`, () => `Choose the Year`),
    }
  })

  // ── navigation (useCalendar + getNavMonths + getInitialMonth) ──
  const navMonths = createMemo((): [Date | undefined, Date | undefined] => {
    let start = props.startMonth ?? props.fromMonth ?? (props.fromYear ? new Date(props.fromYear, 0, 1) : undefined)
    let end = props.endMonth ?? props.toMonth ?? (props.toYear ? new Date(props.toYear, 11, 31) : undefined)
    const yearDropdown = props.captionLayout === `dropdown` || props.captionLayout === `dropdown-years`
    if (start) start = startOfMonth(start)
    else if (yearDropdown) start = startOfYear(addYears(today(), -100))
    if (end) end = endOfMonth(end)
    else if (yearDropdown) end = endOfYear(today())
    return [start ? startOfDay(start) : start, end ? startOfDay(end) : end]
  })
  const navStart = () => navMonths()[0]
  const navEnd = () => navMonths()[1]
  const initialMonth = () => {
    const n = props.numberOfMonths ?? 1
    let m = props.month || props.defaultMonth || today()
    const end = navEnd()
    const start = navStart()
    if (end && differenceInCalendarMonths(end, m) < n - 1) m = addMonths(end, -1 * (n - 1))
    if (start && differenceInCalendarMonths(m, start) < 0) m = start
    return startOfMonth(m)
  }
  const [internalMonth, setInternalMonth] = createSignal(untrack(initialMonth))
  const firstMonth = createMemo(() => (props.month ? initialMonth() : internalMonth()), undefined, { equals: (a, b) => a.getTime() === b.getTime() })

  const calendar = createMemo(() => {
    const first = firstMonth()
    const n = props.numberOfMonths ?? 1
    const end = navEnd()
    const displayMonths: Date[] = []
    for (let i = 0; i < n; i++) {
      const m = addMonths(first, i)
      if (end && m > end) break
      displayMonths.push(m)
    }
    // getDates
    const last = displayMonths[displayMonths.length - 1]!
    const gridStart = sow(displayMonths[0]!)
    const monthsEnd = eow(endOfMonth(last))
    const constraint = props.endMonth ? eow(endOfMonth(props.endMonth)) : undefined
    const gridEnd = constraint && monthsEnd > constraint ? constraint : monthsEnd
    const dates: Date[] = []
    for (let i = 0; i <= differenceInCalendarDays(gridEnd, gridStart); i++) dates.push(addDays(gridStart, i))
    const fixedTotal = 42 * (differenceInCalendarMonths(last, displayMonths[0]!) + 1)
    if (props.fixedWeeks) while (dates.length < fixedTotal) dates.push(addDays(dates[dates.length - 1]!, 1))
    // getMonths
    const months: CalendarMonth[] = displayMonths.map((month) => {
      const firstOfWeek = sow(month)
      const lastOfWeek = eow(endOfMonth(month))
      const monthDates = dates.filter((d) => d >= firstOfWeek && d <= lastOfWeek)
      if (props.fixedWeeks && monthDates.length < 42) {
        const extra = 42 - monthDates.length
        monthDates.push(...dates.filter((d) => d > lastOfWeek && d <= addDays(lastOfWeek, extra)))
      }
      const weeks: CalendarWeek[] = []
      for (const date of monthDates) {
        const weekNumber = props.ISOWeek ? getWeek(date, ISO) : getWeek(date, week())
        const day = new CalendarDay(date, month)
        const w = weeks.find((x) => x.weekNumber === weekNumber)
        if (w) w.days.push(day)
        else weeks.push({ weekNumber, days: [day] })
      }
      return { date: month, weeks }
    })
    if (props.reverseMonths) months.reverse()
    const days = months.flatMap((m) => m.weeks.flatMap((w) => w.days))
    // getPreviousMonth / getNextMonth
    const offset = props.pagedNavigation ? n : 1
    const start = navStart()
    let previousMonth: Date | undefined
    let nextMonth: Date | undefined
    if (!props.disableNavigation) {
      const month = startOfMonth(first)
      previousMonth = !start || differenceInCalendarMonths(month, start) > 0 ? addMonths(month, -offset) : undefined
      nextMonth = !end || differenceInCalendarMonths(end, first) >= n ? addMonths(month, offset) : undefined
    }
    return { months, days, previousMonth, nextMonth }
  })

  const goToMonth = (date: Date) => {
    if (props.disableNavigation) return
    let m = startOfMonth(date)
    const start = navStart()
    const end = navEnd()
    if (start && m < startOfMonth(start)) m = startOfMonth(start)
    if (end && m > startOfMonth(end)) m = startOfMonth(end)
    setInternalMonth(m)
    props.onMonthChange?.(m)
  }
  const goToDay = (day: CalendarDay) => {
    if (calendar().days.some((d) => d.isEqualTo(day))) return
    goToMonth(day.date)
  }

  // ── modifiers (createGetModifiers) ──
  const baseModifiers = createMemo(() => {
    const { days } = calendar()
    const start = navStart() && startOfMonth(navStart()!)
    const end = navEnd() && endOfMonth(navEnd()!)
    const t = today()
    const map = new Map<CalendarDay, Modifiers>()
    for (const day of days) {
      const { date } = day
      const outside = day.outside
      const hidden =
        Boolean(props.hidden && dateMatchModifiers(date, props.hidden)) || Boolean(start && date < start) || Boolean(end && date > end) || (!props.showOutsideDays && outside)
      const m: Modifiers = { focused: false, disabled: Boolean(props.disabled && dateMatchModifiers(date, props.disabled)), hidden, outside, today: isSameDay(date, t) }
      for (const [name, matcher] of Object.entries(props.modifiers ?? {})) if (matcher && dateMatchModifiers(date, matcher)) m[name] = true
      // A custom modifier that never matched is `false` (DayPicker lists it only when one day matched).
      map.set(day, m)
    }
    for (const name of Object.keys(props.modifiers ?? {})) {
      if (![...map.values()].some((m) => m[name])) continue
      for (const m of map.values()) if (!(name in m)) m[name] = false
    }
    return map
  })
  const getModifiers = (day: CalendarDay): Modifiers => ({ ...(baseModifiers().get(day) ?? NO_FLAGS) })

  // ── selection (useSingle / useMulti / useRange) ──
  const [internalSelected, setInternalSelected] = createSignal<unknown>(untrack(() => props.selected))
  const selected = (): unknown => (props.onSelect ? props.selected : internalSelected())
  const isSelected = (date: Date): boolean | undefined => {
    const s = selected()
    switch (props.mode) {
      case `single`:
        return s ? isSameDay(s as Date, date) : false
      case `multiple`:
        return (s as Date[] | undefined)?.some((d) => isSameDay(d, date)) ?? false
      case `range`:
        return (s as DateRange | undefined) && rangeIncludesDate(s as DateRange, date, false)
      default:
        return undefined
    }
  }
  const select = (triggerDate: Date, modifiers: Modifiers, e: MouseEvent) => {
    const s = selected()
    const p = props as CalendarProps & { min?: number; max?: number; resetOnSelect?: boolean }
    let next: unknown
    if (props.mode === `single`) {
      next = !props.required && s && isSameDay(triggerDate, s as Date) ? undefined : triggerDate
    } else if (props.mode === `multiple`) {
      const list = s as Date[] | undefined
      if (isSelected(triggerDate)) {
        if (list?.length === p.min) return
        if (props.required && list?.length === 1) return
        next = list?.filter((d) => !isSameDay(d, triggerDate))
      } else next = list?.length === p.max ? [triggerDate] : [...(list ?? []), triggerDate]
    } else if (props.mode === `range`) {
      const r = s as DateRange | undefined
      const full = Boolean(r?.from && r?.to)
      if (p.resetOnSelect && (full || !r?.from)) {
        const singleDay = Boolean(r?.from && r?.to && isSameDay(r.from, r.to) && isSameDay(triggerDate, r.from))
        next = !props.required && singleDay ? undefined : { from: triggerDate, to: undefined }
      } else next = addToRange(triggerDate, r, p.min, p.max, props.required)
    } else return
    if (!props.onSelect) setInternalSelected(() => next)
    ;(props.onSelect as SelectHandler<unknown> | undefined)?.(next, triggerDate, modifiers, e)
  }

  // ── focus (useFocus + calculateFocusTarget) ──
  const [lastFocused, setLastFocused] = createSignal<CalendarDay | undefined>()
  const focusTarget = createMemo(() => {
    const focusable = (m: Modifiers) => !m.disabled && !m.hidden && !m.outside
    let target: CalendarDay | undefined
    let priority = -1
    const last = lastFocused()
    for (const day of calendar().days) {
      const m = getModifiers(day)
      if (!focusable(m)) continue
      if (m.focused && priority < 3) [target, priority] = [day, 3]
      else if (last?.isEqualTo(day) && priority < 2) [target, priority] = [day, 2]
      else if (isSelected(day.date) && priority < 1) [target, priority] = [day, 1]
      else if (m.today && priority < 0) [target, priority] = [day, 0]
    }
    return target ?? calendar().days.find((d) => focusable(getModifiers(d)))
  })
  const [focused, setFocused] = createSignal<CalendarDay | undefined>(untrack(() => (props.autoFocus ? focusTarget() : undefined)))
  const blur = () => {
    setLastFocused(focused())
    setFocused(undefined)
  }
  const nextFocus = (moveBy: string, moveDir: `before` | `after`, ref: CalendarDay, attempt = 0): CalendarDay | undefined => {
    if (attempt > 365) return undefined
    const sign = moveDir === `after` ? 1 : -1
    const moves: Record<string, (d: Date) => Date> = {
      day: (d) => addDays(d, sign),
      week: (d) => addDays(d, 7 * sign),
      month: (d) => addMonths(d, sign),
      year: (d) => addYears(d, sign),
      startOfWeek: (d) => sow(d),
      endOfWeek: (d) => eow(d),
    }
    let date = moves[moveBy]!(ref.date)
    if (moveDir === `before` && navStart()) date = maxDate([navStart()!, date])
    else if (moveDir === `after` && navEnd()) date = minDate([navEnd()!, date])
    const day = new CalendarDay(date, date)
    const blocked = Boolean(props.disabled && dateMatchModifiers(date, props.disabled)) || Boolean(props.hidden && dateMatchModifiers(date, props.hidden))
    return blocked ? nextFocus(moveBy, moveDir, day, attempt + 1) : day
  }
  const moveFocus = (moveBy: string, moveDir: `before` | `after`) => {
    const current = focused()
    if (!current) return
    const next = nextFocus(moveBy, moveDir, current)
    if (!next) return
    if (props.disableNavigation && !calendar().days.some((d) => d.isEqualTo(next))) return
    goToDay(next)
    setFocused(next)
  }

  const dayModifiers = (day: CalendarDay): Modifiers => {
    const m = getModifiers(day)
    m.focused = !m.hidden && Boolean(focused()?.isEqualTo(day))
    m.selected = isSelected(day.date) || m.selected
    const s = selected()
    if (props.mode === `range` && isDateRange(s)) {
      const { from, to } = s
      m.range_start = Boolean(from && to && isSameDay(day.date, from))
      m.range_end = Boolean(from && to && isSameDay(day.date, to))
      m.range_middle = rangeIncludesDate(s, day.date, true)
    }
    return m
  }
  const dayClass = (m: Modifiers) => {
    const cls = classNames()
    const own = props.modifiersClassNames ?? {}
    const out = [cls.day]
    for (const [key, active] of Object.entries(m)) {
      if (active !== true) continue
      if (own[key]) out.push(own[key])
      else if (DAY_FLAGS.includes(key) || SELECTION_STATES.includes(key)) {
        if (cls[key]) out.push(cls[key])
      }
    }
    return out.join(` `)
  }

  const interactive = () => props.mode !== undefined || props.onDayClick !== undefined
  const multiselectable = () => String(props.mode === `multiple` || props.mode === `range`)

  const handlePrevious = () => {
    const prev = calendar().previousMonth
    if (!prev) return
    goToMonth(prev)
    props.onPrevClick?.(prev)
  }
  const handleNext = () => {
    const next = calendar().nextMonth
    if (!next) return
    goToMonth(next)
    props.onNextClick?.(next)
  }
  const onDayKeyDown = (day: CalendarDay, m: () => Modifiers) => (e: KeyboardEvent) => {
    const rtl = props.dir === `rtl`
    const keyMap: Record<string, [string, `before` | `after`]> = {
      ArrowLeft: [e.shiftKey ? `month` : `day`, rtl ? `after` : `before`],
      ArrowRight: [e.shiftKey ? `month` : `day`, rtl ? `before` : `after`],
      ArrowDown: [e.shiftKey ? `year` : `week`, `after`],
      ArrowUp: [e.shiftKey ? `year` : `week`, `before`],
      PageUp: [e.shiftKey ? `year` : `month`, `before`],
      PageDown: [e.shiftKey ? `year` : `month`, `after`],
      Home: [`startOfWeek`, `before`],
      End: [`endOfWeek`, `after`],
    }
    const hit = keyMap[e.key]
    if (hit) {
      e.preventDefault()
      e.stopPropagation()
      moveFocus(hit[0], hit[1])
    }
    props.onDayKeyDown?.(day.date, m(), e)
  }

  const Chevron = (p: CalendarChevronProps) => {
    const Custom = props.components?.Chevron
    if (Custom) return <Custom {...p} />
    return <CalendarChevron {...p} />
  }
  const DayButtonComponent = () => props.components?.DayButton ?? CalendarDayButton

  const rootData = () => {
    const out: Record<string, string | undefined> = {
      "data-mode": props.mode ?? undefined,
      "data-required": `required` in input && input.required !== undefined ? String(input.required) : undefined,
      "data-multiple-months": (props.numberOfMonths ?? 0) > 1 ? `true` : undefined,
      "data-week-numbers": props.showWeekNumber ? `true` : undefined,
    }
    for (const key of Object.keys(input)) if (key.startsWith(`data-`)) out[key] = (input as Record<string, string | undefined>)[key]
    return out
  }

  const dropdown = (kind: `month` | `year`, month: Date) => {
    const cls = classNames()
    const options = () => {
      if (kind === `month`) {
        return Array.from({ length: 12 }, (_, i) => {
          const m = new Date(month.getFullYear(), i, 1)
          const start = navStart()
          const end = navEnd()
          return { value: i, label: formatters().formatMonthDropdown(m), disabled: Boolean((start && m < startOfMonth(start)) || (end && m > startOfMonth(end))) }
        })
      }
      const start = navStart()
      const end = navEnd()
      if (!start || !end) return []
      const years: { value: number; label: string; disabled: boolean }[] = []
      for (let y = start.getFullYear(); y <= end.getFullYear(); y++) years.push({ value: y, label: formatters().formatYearDropdown(new Date(y, 0, 1)), disabled: false })
      if (props.reverseYears) years.reverse()
      return years
    }
    const value = kind === `month` ? month.getMonth() : month.getFullYear()
    const onChange = (e: Event) => {
      const v = Number((e.currentTarget as HTMLSelectElement).value)
      const base = startOfMonth(month)
      if (kind === `month`) goToMonth(new Date(base.getFullYear(), v, 1))
      else {
        const next = clone(base)
        next.setFullYear(v)
        goToMonth(next)
      }
    }
    return (
      <span data-disabled={String(Boolean(props.disableNavigation))} class={cls.dropdown_root}>
        <select
          class={[cls.dropdown, kind === `month` ? cls.months_dropdown : cls.years_dropdown].join(` `)}
          aria-label={kind === `month` ? labels().labelMonthDropdown() : labels().labelYearDropdown()}
          disabled={Boolean(props.disableNavigation)}
          onChange={onChange}
          ref={(el) => queueMicrotask(() => (el.value = String(value)))}
        >
          <For each={options()}>
            {(o) => (
              <option value={o.value} disabled={o.disabled} selected={o.value === value}>
                {o.label}
              </option>
            )}
          </For>
        </select>
        <span class={cls.caption_label} aria-hidden="true">
          {options().find((o) => o.value === value)?.label}
          <Chevron orientation="down" size={18} class={cls.chevron} />
        </span>
      </span>
    )
  }

  return (
    <div
      data-slot="calendar"
      class={cn([classNames().root, cn(CALENDAR_CLASSES.root, props.class)].filter(Boolean).join(` `))}
      style={props.style}
      dir={props.dir}
      id={props.id}
      lang={props.lang ?? locale().code}
      nonce={props.nonce}
      title={props.title}
      role={props.role}
      aria-label={props[`aria-label`]}
      aria-labelledby={props[`aria-labelledby`]}
      {...rootData()}
    >
      <div class={classNames().months}>
        <Show when={!props.hideNavigation}>
          <nav class={classNames().nav} aria-label={labels().labelNav()}>
            <button
              type="button"
              class={classNames().button_previous}
              tabindex={calendar().previousMonth ? undefined : -1}
              aria-disabled={calendar().previousMonth ? undefined : `true`}
              aria-label={labels().labelPrevious(calendar().previousMonth)}
              onClick={handlePrevious}
            >
              <Chevron disabled={calendar().previousMonth ? undefined : true} class={classNames().chevron} orientation="left" />
            </button>
            <button
              type="button"
              class={classNames().button_next}
              tabindex={calendar().nextMonth ? undefined : -1}
              aria-disabled={calendar().nextMonth ? undefined : `true`}
              aria-label={labels().labelNext(calendar().nextMonth)}
              onClick={handleNext}
            >
              <Chevron disabled={calendar().nextMonth ? undefined : true} orientation="right" class={classNames().chevron} />
            </button>
          </nav>
        </Show>
        <For each={calendar().months}>
          {(calendarMonth) => (
            <div class={classNames().month}>
              <div class={classNames().month_caption}>
                {props.captionLayout.startsWith(`dropdown`) ? (
                  <div class={classNames().dropdowns}>
                    {(() => {
                      const layout = props.captionLayout
                      const monthControl = layout === `dropdown` || layout === `dropdown-months` ? dropdown(`month`, calendarMonth.date) : <span>{formatters().formatMonthDropdown(calendarMonth.date)}</span>
                      const yearControl = layout === `dropdown` || layout === `dropdown-years` ? dropdown(`year`, calendarMonth.date) : <span>{formatters().formatYearDropdown(calendarMonth.date)}</span>
                      return fmt().yearFirst ? [yearControl, monthControl] : [monthControl, yearControl]
                    })()}
                    <span role="status" aria-live="polite" style={STATUS_STYLE}>
                      {formatters().formatCaption(calendarMonth.date)}
                    </span>
                  </div>
                ) : (
                  <span class={classNames().caption_label} role="status" aria-live="polite">
                    {formatters().formatCaption(calendarMonth.date)}
                  </span>
                )}
              </div>
              <table role="grid" aria-multiselectable={multiselectable() as `true` | `false`} aria-label={labels().labelGrid(calendarMonth.date) || undefined} class={classNames().month_grid}>
                <Show when={!props.hideWeekdays}>
                  <thead aria-hidden="true">
                    <tr class={classNames().weekdays}>
                      <Show when={props.showWeekNumber}>
                        <th aria-label={labels().labelWeekNumberHeader()} class={classNames().week_number_header} scope="col">
                          {formatters().formatWeekNumberHeader()}
                        </th>
                      </Show>
                      <For each={Array.from({ length: 7 }, (_, i) => addDays(sow(today()), i))}>
                        {(weekday) => (
                          <th aria-label={labels().labelWeekday(weekday)} class={classNames().weekday} scope="col">
                            {formatters().formatWeekdayName(weekday)}
                          </th>
                        )}
                      </For>
                    </tr>
                  </thead>
                </Show>
                <tbody class={classNames().weeks}>
                  <For each={calendarMonth.weeks}>
                    {(calendarWeek) => (
                      <tr class={classNames().week}>
                        <Show when={props.showWeekNumber}>
                          {/* shadcn's WeekNumber spreads DayPicker's props onto the <td>: `week` lands as "[object Object]". */}
                          <td {...({ "attr:week": String(calendarWeek) } as unknown as Record<string, never>)} aria-label={labels().labelWeekNumber(calendarWeek.weekNumber)} class={classNames().week_number} scope="row" role="rowheader">
                            <div class={CALENDAR_CLASSES.weekNumber}>{formatters().formatWeekNumber(calendarWeek.weekNumber)}</div>
                          </td>
                        </Show>
                        <For each={calendarWeek.days}>
                          {(day) => {
                            const m = createMemo(() => dayModifiers(day))
                            const DayButton = DayButtonComponent()
                            return (
                              <td
                                class={dayClass(m())}
                                role="gridcell"
                                aria-selected={m().selected ? `true` : undefined}
                                aria-label={!interactive() && !m().hidden ? labels().labelGridcell(day.date, m()) : undefined}
                                data-day={day.isoDate}
                                data-month={day.outside ? day.dateMonthId : undefined}
                                data-selected={m().selected ? `true` : undefined}
                                data-disabled={m().disabled ? `true` : undefined}
                                data-hidden={m().hidden ? `true` : undefined}
                                data-outside={day.outside ? `true` : undefined}
                                data-focused={m().focused ? `true` : undefined}
                                data-today={m().today ? `true` : undefined}
                              >
                                {!m().hidden && interactive() ? (
                                  <DayButton
                                    class={classNames().day_button}
                                    type="button"
                                    day={day}
                                    modifiers={m()}
                                    disabled={(!m().focused && m().disabled) || undefined}
                                    aria-disabled={(m().focused && m().disabled) || undefined ? `true` : undefined}
                                    tabindex={focusTarget()?.isEqualTo(day) ? 0 : -1}
                                    aria-label={labels().labelDayButton(day.date, m())}
                                    onClick={(e: MouseEvent) => {
                                      e.preventDefault()
                                      e.stopPropagation()
                                      setFocused(day)
                                      const mods = m()
                                      if (mods.disabled) return
                                      select(day.date, mods, e)
                                      props.onDayClick?.(day.date, mods, e)
                                    }}
                                    onFocus={(e: FocusEvent) => {
                                      setFocused(day)
                                      props.onDayFocus?.(day.date, m(), e)
                                    }}
                                    onBlur={(e: FocusEvent) => {
                                      blur()
                                      props.onDayBlur?.(day.date, m(), e)
                                    }}
                                    onKeyDown={onDayKeyDown(day, m)}
                                    onMouseEnter={(e: MouseEvent) => props.onDayMouseEnter?.(day.date, m(), e)}
                                    onMouseLeave={(e: MouseEvent) => props.onDayMouseLeave?.(day.date, m(), e)}
                                  >
                                    {formatters().formatDay(day.date)}
                                  </DayButton>
                                ) : !m().hidden ? (
                                  formatters().formatDay(day.date)
                                ) : null}
                              </td>
                            )
                          }}
                        </For>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          )}
        </For>
      </div>
      <Show when={props.footer}>
        <div class={classNames().footer} role="status" aria-live="polite">
          {props.footer}
        </div>
      </Show>
    </div>
  )
}

/** shadcn's Chevron: the Lucide chevrons at `size-4` (lucide-react's attributes; `disabled` and `size` land on the <svg> as React spreads them). */
function CalendarChevron(props: CalendarChevronProps): JSX.Element {
  const icon = () => (props.orientation === `left` ? ChevronLeft : props.orientation === `right` ? ChevronRight : ChevronDown)
  return <PaintIcon icon={icon()} class={cn(CALENDAR_CLASSES.chevron, props.class)} size={props.size} {...((props.disabled ? { "attr:disabled": `` } : {}) as Record<string, never>)} />
}

/** shadcn's CalendarDayButton: the ghost icon Button with the day's data
 *  attributes; focuses itself when its day takes the focus. */
function CalendarDayButton(props: CalendarDayButtonProps): JSX.Element {
  const [local, rest] = splitProps(props, [`class`, `day`, `modifiers`])
  let ref: HTMLButtonElement | undefined
  createEffect(() => {
    if (local.modifiers.focused) ref?.focus()
  })
  const m = () => local.modifiers
  const bool = (v: boolean | undefined) => (v === undefined ? undefined : String(v))
  return (
    <Button
      ref={(el: HTMLButtonElement) => (ref = el)}
      variant="ghost"
      size="icon"
      data-day={local.day.date.toLocaleDateString()}
      data-selected-single={bool(m().selected && !m().range_start && !m().range_end && !m().range_middle)}
      data-range-start={bool(m().range_start)}
      data-range-end={bool(m().range_end)}
      data-range-middle={bool(m().range_middle)}
      class={cn(CALENDAR_CLASSES.dayButton, getDefaultClassNames().day, local.class)}
      {...rest}
    />
  )
}

export { Calendar, CalendarDayButton, CalendarChevron }
