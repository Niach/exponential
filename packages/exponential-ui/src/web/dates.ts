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
