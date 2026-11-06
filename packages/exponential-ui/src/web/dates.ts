// Round 1 (FEED-128: hoisted from the React renderer's natives/dates.tsx):
// the calendar arithmetic of DatePicker / DateRangePicker: ISO day strings,
// the 6×7 month grid and the roving tab stop of a month view.

export const isoDate = (d: Date): string => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, `0`)}-${String(d.getDate()).padStart(2, `0`)}`
export const parseIsoDate = (s: string): Date | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s)
  return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])) : null
}
export const addDays = (d: Date, n: number): Date => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n)
/** `n` months on, the day clamped to the target month's last. */
export const addMonths = (d: Date, n: number): Date => {
  const last = new Date(d.getFullYear(), d.getMonth() + n + 1, 0).getDate()
  return new Date(d.getFullYear(), d.getMonth() + n, Math.min(d.getDate(), last))
}
export const sameDay = (a: Date | null, b: Date | null): boolean => Boolean(a && b && isoDate(a) === isoDate(b))

/** The 6×7 days of `view`'s month, starting on `first` (0 = Sunday). */
export function monthGrid(view: Date, first: number): Date[] {
  const start = new Date(view.getFullYear(), view.getMonth(), 1)
  const lead = (start.getDay() - first + 7) % 7
  return Array.from({ length: 42 }, (_, i) => new Date(start.getFullYear(), start.getMonth(), 1 - lead + i))
}

/** The roving tab stop of a month view: the focused day when it is in the
 *  visible month and enabled, else the enabled in-month day nearest to it,
 *  else none (a disabled day never holds the stop). */
export function calendarStop(days: readonly Date[], view: Date, focus: Date, disabled: (d: Date) => boolean): Date | null {
  const inMonth = days.filter((d) => d.getMonth() === view.getMonth() && d.getFullYear() === view.getFullYear() && !disabled(d))
  if (inMonth.some((d) => sameDay(d, focus))) return focus
  let best: Date | null = null
  for (const d of inMonth) if (!best || Math.abs(d.getTime() - focus.getTime()) < Math.abs(best.getTime() - focus.getTime())) best = d
  return best
}

/** #VAPP-282: one event dot of a month grid (`DatePicker` `marks`). */
export interface DateMark {
  date: string
  tone?: string
}

/** #VAPP-282 (fixtures/month-grid.json): the tone of each grid day's dot,
 *  keyed by ISO day: the FIRST mark of a day wins, no tone = `primary`. */
export function markTones(marks: unknown): Map<string, string> {
  const out = new Map<string, string>()
  if (!Array.isArray(marks)) return out
  for (const m of marks) {
    if (!m || typeof m !== `object`) continue
    const date = typeof (m as DateMark).date === `string` ? (m as DateMark).date.slice(0, 10) : ``
    if (!parseIsoDate(date) || out.has(date)) continue
    const tone = (m as DateMark).tone
    out.set(date, typeof tone === `string` && tone ? tone : `primary`)
  }
  return out
}

/** The month a grid opens on: `value`'s, else `today`'s, else `min`'s, else
 *  the device's (the first of that month). */
export function openingMonth(value: string, today: string, min: string): Date {
  const d = parseIsoDate(value) ?? parseIsoDate(today) ?? parseIsoDate(min) ?? new Date()
  return new Date(d.getFullYear(), d.getMonth(), 1)
}

/** `yyyy-mm` of a month (the `month` event's payload). */
export const isoMonth = (d: Date): string => isoDate(d).slice(0, 7)

/** One cell of a month view. */
export interface MonthCell {
  date: string
  day: number
  outside: boolean
  today: boolean
  selected: boolean
  mark: string | null
}

/** THE month view (fixtures/month-grid.json): 42 cells of `view`'s month
 *  from `first`, flagged outside / today / selected, with their dot. */
export function monthView(view: Date, first: number, opts: { value?: string; today?: string; marks?: unknown }): MonthCell[] {
  const tones = markTones(opts.marks)
  return monthGrid(view, first).map((d) => {
    const date = isoDate(d)
    return { date, day: d.getDate(), outside: d.getMonth() !== view.getMonth(), today: date === opts.today, selected: date === opts.value, mark: tones.get(date) ?? null }
  })
}
