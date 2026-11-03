// eApps U3 (VAPP-232, spec §17 U3): the CALENDAR half of the records query
// (fixtures/records-query.json `calendarCases`): the month view's weeks with
// their lanes, the week view and the timeline. Pure day maths over day
// numbers (`../dates.ts`), so the Rust core's `host/calendar.rs` mirrors it
// line by line. query.ts filters and sorts the rows, then hands each placed
// row here as a span (its first and last day, its tone, its value row).

import { dayFromNumber, dayNumber, formatDay, parseDay, type CalendarDay } from "../dates"

/** The lanes a month or week shows per week before "+N more" (1–8). */
export const CALENDAR_LANES_DEFAULT = 3
export const CALENDAR_LANES_MAX = 8
/** The timeline's columns (1–52) and scales. */
export const TIMELINE_COLUMNS_DEFAULT = 12
export const TIMELINE_COLUMNS_MAX = 52
export type TimelineScale = `week` | `month` | `quarter`
export const TIMELINE_SCALES: readonly TimelineScale[] = [`week`, `month`, `quarter`]
/** The tones a placed row takes (catalog tokens `$color.chart1..8`). */
export const CALENDAR_TONES: readonly string[] = [`chart1`, `chart2`, `chart3`, `chart4`, `chart5`, `chart6`, `chart7`, `chart8`]
/** Monday first (the calendar's week; `RECORDS_WEEKDAYS` stays Sunday first). */
export const CALENDAR_WEEKDAYS_LONG: readonly string[] = [`Monday`, `Tuesday`, `Wednesday`, `Thursday`, `Friday`, `Saturday`, `Sunday`]
const WEEKDAYS: readonly string[] = [`Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat`, `Sun`]
const MONTHS: readonly string[] = [`January`, `February`, `March`, `April`, `May`, `June`, `July`, `August`, `September`, `October`, `November`, `December`]

/** One placed row: its first and last day (day numbers, inclusive), its
 *  tone, its value row and its place in the query's order. */
export interface CalendarSpan {
  id: string
  start: number
  end: number
  tone: string
  value: Record<string, unknown>
  order: number
}

/** A real day's number; null = not a day. */
export function dayOf(text: string): number | null {
  const d = parseDay(text)
  return d ? dayNumber(d.y, d.m, d.d) : null
}

/** `start`..`end` as a row's span: `end` missing, not a day or before the
 *  start = the start day only; null = the start is no day. */
export function spanOf(startText: string, endText: string | null): { start: number; end: number } | null {
  const start = dayOf(startText)
  if (start === null) return null
  const end = endText === null ? null : dayOf(endText)
  return { start, end: end !== null && end >= start ? end : start }
}

export const dayText = (n: number): string => formatDay(dayFromNumber(n))
/** 0 = Monday (1970-01-01, day 0, was a Thursday). */
const weekdayIndex = (n: number): number => (((n + 3) % 7) + 7) % 7
const mondayOf = (n: number): number => n - weekdayIndex(n)

/** The tones of `values` (a placed row's tone field, as text): an integer
 *  1–8 is that chart tone; any other non-empty text takes the tone of its
 *  rank among the distinct such texts (code units, i → chart(i mod 8 + 1));
 *  empty = chart1. */
export function tonesOf(values: readonly string[]): string[] {
  const named = [...new Set(values.filter((v) => v !== `` && !/^[1-8]$/.test(v)))].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))
  const rank = new Map(named.map((t, i) => [t, i]))
  return values.map((v) => (/^[1-8]$/.test(v) ? `chart${v}` : v === `` ? `chart1` : CALENDAR_TONES[rank.get(v)! % 8]!))
}

/** Spans clipped to `[from, to]` in lane order: by first day, then the
 *  longer first, then the query's order. */
function laneOrder(spans: readonly CalendarSpan[], from: number, to: number): CalendarSpan[] {
  return spans
    .filter((s) => s.end >= from && s.start <= to)
    .sort((a, b) => Math.max(a.start, from) - Math.max(b.start, from) || Math.min(b.end, to) - Math.max(b.start, from) - (Math.min(a.end, to) - Math.max(a.start, from)) || a.order - b.order)
}

/** The lowest lane whose cells `first..last` are all free (`max` lanes;
 *  null = none), marking them taken. */
function takeLane(lanes: boolean[][], first: number, last: number, max: number): number | null {
  for (let lane = 0; lane < max; lane++) {
    const row = (lanes[lane] ??= [])
    let free = true
    for (let c = first; c <= last; c++) if (row[c]) free = false
    if (!free) continue
    for (let c = first; c <= last; c++) row[c] = true
    return lane
  }
  return null
}

export interface CalendarDayCell {
  date: string
  day: number
  weekday: string
  initial: string
  inMonth: boolean
  weekend: boolean
  today: boolean
  selected: boolean
  total: number
  dots: { tone: string }[]
}

export interface CalendarWeek {
  key: string
  days: CalendarDayCell[]
  segments: Record<string, unknown>[]
}

/** One Monday-first week: its 7 days and its segments: every span crossing
 *  it, clipped, in lane order, the first `lanes` lanes shown (`col` 1 =
 *  Monday, `span` days, `contL`/`contR` = it goes on before / after the
 *  week), then one "+N more" marker per day with hidden spans (`more` = N,
 *  lane = `lanes`). */
export function calendarWeek(monday: number, spans: readonly CalendarSpan[], o: { lanes: number; today: number | null; selected: number | null; month: number | null }): CalendarWeek {
  const sunday = monday + 6
  const placed = laneOrder(spans, monday, sunday)
  const lanes: boolean[][] = []
  const hidden = [0, 0, 0, 0, 0, 0, 0]
  const segments: Record<string, unknown>[] = []
  for (const s of placed) {
    const first = Math.max(s.start, monday) - monday
    const last = Math.min(s.end, sunday) - monday
    const lane = takeLane(lanes, first, last, o.lanes)
    if (lane === null) {
      for (let c = first; c <= last; c++) hidden[c]! += 1
      continue
    }
    segments.push({ id: s.id, col: first + 1, span: last - first + 1, lane, contL: s.start < monday, contR: s.end > sunday, tone: s.tone, more: 0, row: s.value })
  }
  hidden.forEach((n, c) => {
    if (n > 0) segments.push({ id: `more:${dayText(monday + c)}`, col: c + 1, span: 1, lane: o.lanes, contL: false, contR: false, tone: null, more: n, row: null })
  })
  const days: CalendarDayCell[] = []
  for (let c = 0; c < 7; c++) {
    const n = monday + c
    const d = dayFromNumber(n)
    const covering = placed.filter((s) => s.start <= n && s.end >= n)
    days.push({
      date: formatDay(d),
      day: d.d,
      weekday: WEEKDAYS[c]!,
      initial: WEEKDAYS[c]!.slice(0, 1),
      inMonth: o.month === null || d.m === o.month,
      weekend: c >= 5,
      today: n === o.today,
      selected: n === o.selected,
      total: covering.length,
      dots: covering.slice(0, 3).map((s) => ({ tone: s.tone })),
    })
  }
  return { key: dayText(monday), days, segments }
}

/** `2 – 8 November 2026`, `30 November – 6 December 2026`, `29 December 2025 – 4 January 2026`. */
export function rangeLabel(from: number, to: number): string {
  const a = dayFromNumber(from)
  const b = dayFromNumber(to)
  const tail = `${b.d} ${MONTHS[b.m - 1]} ${b.y}`
  if (a.y !== b.y) return `${a.d} ${MONTHS[a.m - 1]} ${a.y} – ${tail}`
  if (a.m !== b.m) return `${a.d} ${MONTHS[a.m - 1]} – ${tail}`
  return `${a.d} – ${tail}`
}

/** `Tuesday, 3 November`. */
export function dayLabel(n: number): string {
  const d = dayFromNumber(n)
  return `${CALENDAR_WEEKDAYS_LONG[weekdayIndex(n)]}, ${d.d} ${MONTHS[d.m - 1]}`
}

/** The month view's calendar half: `weeks` (Monday on or before the 1st to
 *  the Sunday on or after the last day), the selected `day` (`day` when it
 *  is in the month, else today when it is, else the 1st) and its `agenda`
 *  (every span covering it, in lane order: {id, tone, start, end, days, row}). */
export function calendarMonth(y: number, m: number, lastDay: number, spans: readonly CalendarSpan[], o: { lanes: number; today: number; day: string | null }): Record<string, unknown> {
  const first = dayNumber(y, m, 1)
  const last = dayNumber(y, m, lastDay)
  const asked = o.day === null ? null : dayOf(o.day)
  const selected = asked !== null && asked >= first && asked <= last ? asked : o.today >= first && o.today <= last ? o.today : first
  const weeks: CalendarWeek[] = []
  for (let monday = mondayOf(first); monday <= last; monday += 7) weeks.push(calendarWeek(monday, spans, { lanes: o.lanes, today: o.today, selected, month: m }))
  const items = laneOrder(spans, selected, selected).map((s) => ({ id: s.id, tone: s.tone, start: dayText(s.start), end: dayText(s.end), days: s.end - s.start + 1, row: s.value }))
  return { weeks, day: dayText(selected), agenda: { date: dayText(selected), label: dayLabel(selected), count: items.length, items } }
}

/** The week view: the Monday-first week holding `key` (a day number). */
export function calendarWeekView(key: number, spans: readonly CalendarSpan[], o: { lanes: number; today: number }): Record<string, unknown> {
  const monday = mondayOf(key)
  const week = calendarWeek(monday, spans, { lanes: o.lanes, today: o.today, selected: null, month: null })
  return { key: week.key, label: rangeLabel(monday, monday + 6), prev: dayText(monday - 7), next: dayText(monday + 7), today: dayText(o.today), days: week.days, segments: week.segments }
}

/** The first day of the scale period holding day `n`. */
export function periodStart(n: number, scale: TimelineScale): number {
  if (scale === `week`) return mondayOf(n)
  const d = dayFromNumber(n)
  return dayNumber(d.y, scale === `month` ? d.m : Math.floor((d.m - 1) / 3) * 3 + 1, 1)
}

/** `start` (a period's first day) moved by `k` periods. */
export function shiftPeriod(start: number, scale: TimelineScale, k: number): number {
  if (scale === `week`) return start + 7 * k
  const d = dayFromNumber(start)
  const index = d.y * 12 + (d.m - 1) + k * (scale === `month` ? 1 : 3)
  return dayNumber(Math.floor(index / 12), (((index % 12) + 12) % 12) + 1, 1)
}

function columnLabel(d: CalendarDay, scale: TimelineScale): string {
  const short = MONTHS[d.m - 1]!.slice(0, 3)
  return scale === `week` ? `${d.d} ${short}` : scale === `month` ? short : `Q${Math.floor((d.m - 1) / 3) + 1}`
}

/** The timeline: `columns` scale periods from the one holding `start`, the
 *  `bands` above them (a week scale: the months, else the years; each
 *  {label, col, span}, a month's label + its year when the axis starts in
 *  another year), and `rows` (by `rowKey`'s text in the query's order of
 *  first appearance; null = one row per span, keyed by its id), each
 *  {key, index, lanes, count, items: [{id, rowIndex, col, span, lane,
 *  contL, contR, tone, row}]} with every lane used (unbounded); `now` =
 *  today's {col, percent} on the axis (columns equal-width), null outside. */
export function calendarTimeline(start: number, spans: readonly (CalendarSpan & { rowKey: string | null })[], o: { scale: TimelineScale; columns: number; today: number }): Record<string, unknown> {
  const p0 = periodStart(start, o.scale)
  const starts: number[] = []
  for (let i = 0; i <= o.columns; i++) starts.push(shiftPeriod(p0, o.scale, i))
  const axisEnd = starts[o.columns]! - 1
  const colOf = (n: number): number => {
    let c = 0
    while (c + 1 < o.columns && starts[c + 1]! <= n) c++
    return c
  }
  const axisYear = dayFromNumber(p0).y
  const columns = starts.slice(0, o.columns).map((s, i) => ({ key: dayText(s), label: columnLabel(dayFromNumber(s), o.scale), end: dayText(starts[i + 1]! - 1), today: o.today >= s && o.today < starts[i + 1]! }))
  const bands: { label: string; col: number; span: number }[] = []
  starts.slice(0, o.columns).forEach((s, i) => {
    const d = dayFromNumber(s)
    const label = o.scale === `week` ? (d.y === axisYear ? MONTHS[d.m - 1]! : `${MONTHS[d.m - 1]} ${d.y}`) : String(d.y)
    const last = bands[bands.length - 1]
    if (last && last.label === label) last.span += 1
    else bands.push({ label, col: i + 1, span: 1 })
  })
  const rows: { key: string; index: number; lanes: number; count: number; items: Record<string, unknown>[] }[] = []
  const byKey = new Map<string, (CalendarSpan & { rowKey: string | null })[]>()
  for (const s of [...spans].sort((a, b) => a.order - b.order)) {
    if (s.end < p0 || s.start > axisEnd) continue
    const key = s.rowKey ?? s.id
    const list = byKey.get(key)
    if (list) list.push(s)
    else byKey.set(key, [s])
  }
  for (const [key, list] of byKey) {
    const index = rows.length
    const lanes: boolean[][] = []
    const items: Record<string, unknown>[] = []
    const cells = list
      .map((s) => ({ s, first: colOf(Math.max(s.start, p0)), last: colOf(Math.min(s.end, axisEnd)) }))
      .sort((a, b) => a.first - b.first || b.last - b.first - (a.last - a.first) || a.s.order - b.s.order)
    for (const { s, first, last } of cells) {
      const lane = takeLane(lanes, first, last, Number.MAX_SAFE_INTEGER)!
      items.push({ id: s.id, rowIndex: index, col: first + 1, span: last - first + 1, lane, contL: s.start < p0, contR: s.end > axisEnd, tone: s.tone, row: s.value })
    }
    rows.push({ key, index, lanes: Math.max(1, lanes.length), count: items.length, items })
  }
  let now: { col: number; percent: number } | null = null
  if (o.today >= p0 && o.today <= axisEnd) {
    const c = colOf(o.today)
    const frac = (o.today - starts[c]!) / (starts[c + 1]! - starts[c]!)
    now = { col: c + 1, percent: Math.round(((c + frac) / o.columns) * 10000) / 100 }
  }
  return {
    scale: o.scale,
    key: dayText(p0),
    label: rangeLabel(p0, axisEnd),
    prev: dayText(shiftPeriod(p0, o.scale, -o.columns)),
    next: dayText(starts[o.columns]!),
    today: dayText(o.today),
    columns,
    bands,
    rows,
    now,
  }
}
